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
mod id;
mod manager;
mod process;
mod processor;
mod signal;
mod switch;
#[allow(clippy::module_inception)]
mod task;

use self::id::TaskUserRes;
use crate::fs::{open_file, OpenFlags};
use crate::logger::{self, info, warn};
use crate::sync::UPSafeCell;
use crate::timer::get_time_ms;
use crate::trap::TrapContext;
use alloc::{sync::Arc, vec::Vec};
use lazy_static::*;
use manager::fetch_task;
use process::ProcessControlBlock;
use switch::__switch;

pub use action::{SignalAction, SignalActions};
pub use context::TaskContext;
pub use id::{kstack_alloc, pid_alloc, KernelStack, PidHandle, IDLE_PID};
pub use manager::{add_task, pid2process, remove_from_pid2process, remove_task};
pub use processor::{
    current_kstack_top, current_process, current_task, current_trap_cx, current_trap_cx_user_va, current_user_token,
    run_tasks, schedule, take_current_task, Processor,
};
pub use signal::{SignalFlags, MAX_SIG};
pub use task::{TaskControlBlock, TaskStatus};

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

pub fn block_current_and_run_next() {
    let task = take_current_task().unwrap();
    let mut task_inner = task.inner_exclusive_access();
    let task_cx_ptr = &mut task_inner.task_cx as *mut TaskContext;
    task_inner.task_status = TaskStatus::Blocked;
    drop(task_inner);
    schedule(task_cx_ptr);
}

/// Exit the current 'Running' task and run the next task in task list.
pub fn exit_current_and_run_next(exit_code: i32) {
    let task = take_current_task().unwrap();
    let mut task_inner = task.inner_exclusive_access();
    let process = task.process.upgrade().unwrap();
    let tid = task_inner.res.as_ref().unwrap().tid;
    // record exit code
    task_inner.exit_code = Some(exit_code);
    task_inner.res = None;
    // 这里我们不直接删除线程, 是因为还有此线程的内核栈仍在用
    // 这部分(内核栈)会在call 系统调用 sys_waittid 被删除
    drop(task_inner);
    drop(task);
    // 如果是当前进程的主线程, 则此进程立即终止
    if tid == 0 {
        let pid = process.getpid();
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
        remove_from_pid2process(pid);
        let mut process_inner = process.inner_exclusive_access();
        // 标记此进程已经是僵尸进程
        process_inner.is_zombie = true;
        // 记录此进程的退出码
        process_inner.exit_code = exit_code;
        // do not move to its parent but under initproc
        {
            let mut initproc_inner = INITPROC.inner_exclusive_access();
            for child in process_inner.children.iter() {
                child.inner_exclusive_access().parent = Some(Arc::downgrade(&INITPROC));
                initproc_inner.children.push(child.clone());
            }
        }

        // 释放此进程下的所有线程对应的用户资源 (包括 tid/trap上下文/用户栈)
        // 这些事情必须在释放进程内存地址空间之前做完
        // 不然它们(用户资源)会被释放两次
        let mut recycle_res = Vec::<TaskUserRes>::new();
        for task in process_inner.tasks.iter().filter(|t| t.is_some()) {
            let task = task.as_ref().unwrap();
            // if other tasks are Ready in TaskManager or waiting for a timer to be
            // expired, we should remove them.
            //
            // Mention that we do not need to consider Mutex/Semaphore since they
            // are limited in a single process. Therefore, the blocked tasks are
            // removed when the PCB is deallocated.
            remove_inactive_task(Arc::clone(&task));
            let mut task_inner = task.inner_exclusive_access();
            if let Some(res) = task_inner.res.take() {
                recycle_res.push(res);
            }
        }
        // dealloc_tid and dealloc_user_res require access to PCB inner, so we
        // need to collect those user res first, then release process_inner
        // for now to avoid deadlock/double borrow problem.
        drop(process_inner);
        recycle_res.clear();

        let mut process_inner = process.inner_exclusive_access();
        process_inner.children.clear();
        // 释放整个进程的内存地址空间 包括程序数据段/程序代码段
        process_inner.memory_set.recycle_data_pages();
        process_inner.fd_table.clear();
        // 删除所有线程
        process_inner.tasks.clear(); //向量清除 意味着里面的元素也drop掉了
    }

    drop(process);
    // we do not have to save task context
    let mut _unused = TaskContext::zero_init();
    schedule(&mut _unused as *mut _);
}

lazy_static! {
    //Global process that init user shell
    pub static ref INITPROC: Arc<ProcessControlBlock> = {
        let inode = open_file("initproc", OpenFlags::RDONLY).unwrap();
        let v = inode.read_all();
        ProcessControlBlock::new(v.as_slice())
    };
}
///Add init process to the manager
pub fn add_initproc() {
    let _initproc = INITPROC.clone();
}

/// 检查有没有kernel层发送的致命错误的信号
pub fn check_signals_error_of_current() -> Option<(i32, &'static str)> {
    let process = current_process();
    let process_inner = process.inner_exclusive_access();

    return process_inner.signals.check_error();
}

pub fn current_add_signal(signal: SignalFlags) {
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    process_inner.signals |= signal; //运算符将左操作数与右操作数按位或, 并将结果分配回左操作数
                                     //ex: 假设x为6 , 二级制x就是000110 x|=1  之后, x就会000111
                                     //  000110
                                     //  000001
                                     //=>000111
    println!(
        "\x1b[38;5;208m[TRAP] 现在进程[{}] 插入信号[{:?}] \x1b[0m",
        process.getpid(),
        process_inner.signals
    );
}

fn call_kernel_signal_handler(signal: SignalFlags) {
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    match signal {
        SignalFlags::SIGSTOP => {
            process_inner.frozen = true;
            process_inner.signals ^= SignalFlags::SIGSTOP; //进行按位异或并将结果存储回
                                                           // ex : x = 1010b x ^= 1100b  结果为0110
                                                           //  1010
                                                           //  1100
                                                           //=>0110
                                                           // 这意味着清除掉已经存在的SIG_STOP信号, 防止它们再次被处理.
        }
        SignalFlags::SIGCONT => {
            if process_inner.signals.contains(SignalFlags::SIGCONT) {
                process_inner.signals ^= SignalFlags::SIGCONT;
                process_inner.frozen = false;
            }
        }
        _ => {
            //SIG_KILL SIG_DEF , 其他信号统一作为kill 信号

            // println!(
            //     "[K] call_kernel_signal_handler:: current task sigflag {:?}",
            //     task_inner.signals
            // );
            process_inner.killed = true;
        }
    }
    println!(
        "\x1b[38;5;208m[TRAP] 关键信号由核心处理 Signal [{:?}] frozen [{}] killed [{}]  \x1b[0m",
        signal, process_inner.frozen, process_inner.killed
    );
}

fn call_user_signal_handler(sig: usize, signal: SignalFlags) {
    //TODO : only main thread should handle this function.
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let task = current_task().unwrap();
    let mut task_inner = task.inner_exclusive_access();

    let handler = process_inner.signal_actions.table[sig].handler;
    if handler != 0 {
        //handle flag
        process_inner.handling_sig = sig as isize;
        process_inner.signals ^= signal;

        //backup trapframe
        let mut trap_ctx = task_inner.get_trap_cx();
        task_inner.trap_ctx_backup = Some(*trap_ctx); //TODO : Some包裹的对象 是不是clone出来的?
                                                      // 最最核心的地方 , 把下一个函数入口指向handler, 此函数的第一个参数a0设置为sig. 执行后下一个进行TRAP
        trap_ctx.sepc = handler;
        //put args (a0) first input param
        trap_ctx.x[10] = sig;
        println!("\x1b[38;5;208m[TRAP] 进程[{}] 调用应用程序自定义的信号回调函数, sig [{}] handler [{}] \x1b[0m",process.getpid(),sig,handler);
    } else {
        // default action
        println!("[K] task/call_user_signal_handler: default action: ignore it or kill process");
    }
}

fn check_pending_signals() {
    for sig in 0..(MAX_SIG + 1) {
        let process = current_process();
        let process_inner = process.inner_exclusive_access();
        let signal = SignalFlags::from_bits(1 << sig).unwrap();
        if process_inner.signals.contains(signal) && (!process_inner.signal_mask.contains(signal)) {
            // 走到这里说明 该进程收到此信号 并且此信号没有被屏蔽
            let mut masked = true;
            let handling_sig = process_inner.handling_sig; //正在执行信号处理的callback函数
            if handling_sig == -1 {
                masked = false;
            } else {
                let handling_sig_usize = handling_sig as usize;
                if !process_inner.signal_actions.table[handling_sig_usize]
                    .mask
                    .contains(signal)
                {
                    masked = false;
                }
            }

            if !masked {
                // 走到这里说明, 该信号既没有被屏蔽, 也没有正在处理中的回调函数
                drop(process_inner);
                drop(process);
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
            let process = current_process();
            let process_inner = process.inner_exclusive_access();
            (process_inner.frozen, process_inner.killed)
        };
        if !frozen || killed {
            // 默认没有frozen 自动从死循环出来
            // 如果进程没有因收到 SIGSTOP 而暂停, 或者收到 SIGKILL 而被杀掉, 那么就可以破坏死循环
            // println!("\x1b[38;5;208m[SYSCALL : signal] 信号 KILLED 从 handle_sginals 死循环中出来 frozen [{}] , killed [{}] \x1b[0m",frozen,killed);
            break;
        }
        // 只有 frozen 且没有killed的进程才会走到这里 陷入死循环.
        // 走到这里 说明还在死循环中, 那么该进程一直处于时间暂停状态 (想到了奇怪的剧情) call sys_yield() 让出CPU给其他进程
        suspend_current_and_run_next();
    }
}

pub fn remove_inactive_task(task: Arc<TaskControlBlock>) {
    remove_task(Arc::clone(&task));
    // remove_timer(Arc::clone(&task)); 暂时不要, 是去除锁用的函数.
}
