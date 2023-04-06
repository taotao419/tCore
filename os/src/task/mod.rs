//! Task management implementation
//!
//! Everything about task management, like starting and switching tasks is
//! implemented here.
//!
//! A single global instance of [`TaskManager`] called `TASK_MANAGER` controls
//! all the tasks in the operating system.
//!
//! Be careful when you see `__switch` ASM function in `switch.S`. Control flow around this function
//! might not be what you expect.

mod action;
mod context;
mod manager;
mod pid;
mod processor;
mod signal;
mod switch;
#[allow(clippy::module_inception)]
mod task;

use crate::fs::{open_file, OpenFlags};
use crate::logger::{self, info, warn};
use crate::sync::UPSafeCell;
use crate::timer::get_time_ms;
use crate::trap::TrapContext;
use alloc::sync::Arc;
use alloc::vec::Vec;
use lazy_static::*;
use switch::__switch;
use task::{TaskControlBlock, TaskStatus};

pub use action::{SignalAction, SignalActions};
pub use context::TaskContext;
pub use manager::{add_task, pid2task};
pub use processor::{
    current_task, current_trap_cx, current_user_token, run_tasks, schedule, take_current_task,
    Processor,
};
pub use signal::{SignalFlags, MAX_SIG};

use self::manager::remove_from_pid2task;

/// Suspend the current 'Running' task and run the next task in task list.
/// aka: sys_yield
pub fn suspend_current_and_run_next() {
    let task = take_current_task().unwrap();

    //-- access current TCB exclusively
    let mut task_inner = task.inner_exclusive_access();
    let task_cx_ptr = &mut task_inner.task_cx as *mut TaskContext;
    //Change status to Ready
    task_inner.task_status = TaskStatus::Ready;
    drop(task_inner);
    // ---- stop exclusively accessing current PCB

    //push back to ready queue
    add_task(task);
    //jump to scheduling cycle
    schedule(task_cx_ptr); //它会调用__switch 汇编函数(核心函数) 真正切换任务
}

/// pid of usertests app in make run TEST=1
pub const IDLE_PID: usize = 0;
/// Exit the current 'Running' task and run the next task in task list.
pub fn exit_current_and_run_next(exit_code: i32) {
    // take from Processor
    let task = take_current_task().unwrap();
    let pid = task.getpid();
    if pid == IDLE_PID {
        println!(
            "[kernel] Idle process exit with exit_code {} ...",
            exit_code
        );
        if exit_code != 0 {
            println!("[kernel] user app exit with exit_code {} ...", exit_code);
            crate::sbi::shutdown()
        } else {
            println!("[kernel] user app exit success ...");
            crate::sbi::shutdown()
        }
    }
    //保存Pid<==>task 的mapping
    remove_from_pid2task(task.getpid());
    let mut inner = task.inner_exclusive_access();
    inner.task_status = TaskStatus::Zombie;
    inner.exit_code = exit_code;
    // do not move to its parent but under initproc

    // ++++++ access initproc TCB exclusively
    {
        let mut initproc_inner = INITPROC.inner_exclusive_access();
        for child in inner.children.iter() {
            child.inner_exclusive_access().parent = Some(Arc::downgrade(&INITPROC));
            initproc_inner.children.push(child.clone());
        }
    }
    // ++++++ release parent PCB

    inner.children.clear();
    inner.memory_set.recycle_data_pages();
    inner.fd_table.clear();
    drop(inner);
    drop(task); //最最关键的一步, task drop掉 也就意味着task queue里面没有此task了
                // we do not have to save task context
    let mut _unused = TaskContext::zero_init();
    schedule(&mut _unused as *mut _);
}

lazy_static! {
    //Global process that init user shell
    pub static ref INITPROC:Arc<TaskControlBlock> = Arc::new({
        let inode=open_file("initproc", OpenFlags::RDONLY).unwrap();
        let v=inode.read_all();
        TaskControlBlock::new(v.as_slice())
    });
}
///Add init process to the manager
pub fn add_initproc() {
    add_task(INITPROC.clone());
}

/// 检查有没有kernel层发送的致命错误的信号
pub fn check_signals_error_of_current() -> Option<(i32, &'static str)> {
    let task = current_task().unwrap();
    let task_inner = task.inner_exclusive_access();

    return task_inner.signals.check_error();
}

pub fn current_add_signal(signal: SignalFlags) {
    let task = current_task().unwrap();
    let mut task_inner = task.inner_exclusive_access();
    task_inner.signals |= signal; //运算符将左操作数与右操作数按位或, 并将结果分配回左操作数
                                  //ex: 假设x为6 , 二级制x就是000110 x|=1  之后, x就会000111
                                  //  000110
                                  //  000001
                                  //=>000111
}

fn call_kernel_signal_handler(signal: SignalFlags) {
    let task = current_task().unwrap();
    let mut task_inner = task.inner_exclusive_access();
    match signal {
        SignalFlags::SIGSTOP => {
            task_inner.frozen = true;
            task_inner.signals ^= SignalFlags::SIGSTOP; //进行按位异或并将结果存储回
                                                        // ex : x = 1010b x ^= 1100b  结果为0110
                                                        //  1010
                                                        //  1100
                                                        //=>0110
                                                        // 这意味着清除掉已经存在的SIG_STOP信号, 防止它们再次被处理.
        }
        SignalFlags::SIGCONT => {
            if task_inner.signals.contains(SignalFlags::SIGCONT) {
                task_inner.signals ^= SignalFlags::SIGCONT;
                task_inner.frozen = false;
            }
        }
        _ => {
            //SIG_KILL SIG_DEF , 其他信号统一作为kill 信号

            // println!(
            //     "[K] call_kernel_signal_handler:: current task sigflag {:?}",
            //     task_inner.signals
            // );
            task_inner.killed = true;
        }
    }
}

fn call_user_signal_handler(sig: usize, signal: SignalFlags) {
    let task = current_task().unwrap();
    let mut task_inner = task.inner_exclusive_access();

    let handler = task_inner.signal_actions.table[sig].handler;
    if handler != 0 {
        //handle flag
        task_inner.handling_sig = sig as isize;
        task_inner.signals ^= signal;

        //backup trapframe
        let mut trap_ctx = task_inner.get_trap_cx();
        task_inner.trap_ctx_backup = Some(*trap_ctx); //TODO : Some包裹的对象 是不是clone出来的?

        trap_ctx.sepc = handler;
        //put args (a0) first input param
        trap_ctx.x[10] = sig;
    } else {
        // default action
        println!("[K] task/call_user_signal_handler: default action: ignore it or kill process");
    }
}

fn check_pending_signals() {
    for sig in 0..(MAX_SIG + 1) {
        let task = current_task().unwrap();
        let task_inner = task.inner_exclusive_access();
        let signal = SignalFlags::from_bits(1 << sig).unwrap();
        if task_inner.signals.contains(signal) && (!task_inner.signal_mask.contains(signal)) {
            // 走到这里说明 该进程收到此信号 并且此信号没有被屏蔽
            let mut masked = true;
            let handling_sig = task_inner.handling_sig; //正在执行信号处理的callback函数
            if handling_sig == -1 {
                masked = false;
            } else {
                let handling_sig_usize = handling_sig as usize;
                if !task_inner.signal_actions.table[handling_sig_usize]
                    .mask
                    .contains(signal)
                {
                    masked = false;
                }
            }

            if !masked {
                // 走到这里说明, 该信号既没有被屏蔽, 也没有正在处理中的回调函数
                drop(task_inner);
                drop(task);
                if signal == SignalFlags::SIGKILL
                    || signal == SignalFlags::SIGSTOP
                    || signal == SignalFlags::SIGCONT
                    || signal == SignalFlags::SIGDEF
                {
                    // 上面4种信号 只能由内核来处理
                    // signal is a kernel signal
                    call_kernel_signal_handler(signal);
                } else {
                    // 其他信号 可以由进程提供的信号处理的 callback函数来处理.
                    // signal is a user signal
                    call_user_signal_handler(sig, signal);
                    return;
                }
            }
        }
    }
}

pub fn handle_signals() {
    loop {
        check_pending_signals();
        let (frozen, killed) = {
            let task = current_task().unwrap();
            let task_inner = task.inner_exclusive_access();
            (task_inner.frozen, task_inner.killed)
        };
        if !frozen || killed {
            // 如果进程没有因收到 SIGSTOP 而暂停, 或者收到 SIGKILL 而被杀掉, 那么就可以破坏死循环
            break;
        }
        // 走到这里 说明还在死循环中, 那么该进程一直处于时间暂停状态 (想到了奇怪的剧情) call sys_yield() 让出CPU给其他进程
        suspend_current_and_run_next();
    }
}
