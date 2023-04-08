//! Process management syscalls
use crate::fs::{list_files, open_file, OpenFlags};
use crate::mm::{translated_ref, translated_refmut, translated_str};
use crate::sbi::shutdown;
use crate::task::{
    add_task, current_task, current_user_token, exit_current_and_run_next, pid2task,
    suspend_current_and_run_next, SignalAction, SignalFlags, MAX_SIG,
};
use crate::timer::get_time_ms;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

/// task exits and submit an exit code
pub fn sys_exit(exit_code: i32) -> ! {
    println!("[kernel] Application exited with code {}", exit_code);
    exit_current_and_run_next(exit_code);
    panic!("Unreachable in sys_exit!");
}

pub fn sys_yield() -> isize {
    suspend_current_and_run_next();
    return 0;
}

/// get time in milliseconds
pub fn sys_get_time() -> isize {
    get_time_ms() as isize
}

pub fn sys_getpid() -> isize {
    current_task().unwrap().pid.0 as isize
}

pub fn sys_getcwd() -> isize {
    let current_task = current_task().unwrap();
    println!("{}", current_task.inner_exclusive_access().working_dir);
    return 0;
}

pub fn sys_chdir(path: *const u8) -> isize {
    let current_task = current_task().unwrap();
    let token = current_user_token();
    let path = translated_str(token, path);
    current_task.inner_exclusive_access().working_dir = path;
    return 0;
}

pub fn sys_fork() -> isize {
    let current_task = current_task().unwrap();
    let new_task = current_task.fork();
    let new_pid = new_task.pid.0;
    // modify trap context of new_task, because it returns immediately after switching
    let trap_cx = new_task.inner_exclusive_access().get_trap_cx();
    // we do not have to move to next instruction since we have done it before
    // for child process, fork returns 0
    trap_cx.x[10] = 0;
    //add new task to scheduler
    add_task(new_task);

    return (new_pid as isize);
}

/// 这里args 指向命令行参数字符串其实地址数组的一个位置
/// 假设传入了三个字符串 "arg1" "arg2" "arg3"
/// ex : args ptr -> |a|r|g|1
///                  |a|r|g|2
///                  |a|r|g|3
pub fn sys_exec(path: *const u8, mut args: *const usize) -> isize {
    let token = current_user_token();
    let path = translated_str(token, path);
    let mut args_vec: Vec<String> = Vec::new();
    loop {
        let arg_str_ptr = *translated_ref(token, args); //args现在指向arg1字符串的数组位置, 但是这个位置是虚拟地址, OS在内核层还需要翻译成物理地址,这里只是读出字符串头字符的指针来.
        if arg_str_ptr == 0 {
            //如果读出的字符串是0, 说明后面再没有参数了
            break;
        }
        args_vec.push(translated_str(token, arg_str_ptr as *const u8)); //OS 真正从物理地址读出参数字符串 并放入Vec
        unsafe {
            args = args.add(1); //指针下移一位, 指向下一个参数字符串
        }
    }

    if let Some(app_inode) = open_file(path.as_str(), OpenFlags::RDONLY) {
        let all_data = app_inode.read_all();
        let task = current_task().unwrap();
        let argc = args_vec.len();
        task.exec(path.as_str(), all_data.as_slice(), args_vec);
        return argc as isize;
    } else {
        return -1;
    }
}

/// If there is not a child process whose pid is same as given, return -1.
/// Else if there is a child process but it is still running, return -2.
pub fn sys_waitpid(pid: isize, exit_code_ptr: *mut i32) -> isize {
    let task = current_task().unwrap();
    // find a child process

    // ---- access current PCB exclusively
    let mut inner = task.inner_exclusive_access();
    if !inner
        .children
        .iter()
        .any(|p| pid == -1 || pid as usize == p.getpid())
    {
        return -1;
        // ---- release current PCB
    }
    let pair = inner.children.iter().enumerate().find(|(_, p)| {
        // ++++ temporarily access child PCB lock exclusively
        p.inner_exclusive_access().is_zombie() && (pid == -1 || pid as usize == p.getpid())
        // ++++ release child PCB
    });
    if let Some((idx, _)) = pair {
        let child = inner.children.remove(idx);
        // confirm that child will be deallocated after removing from children list
        assert_eq!(Arc::strong_count(&child), 1);
        let found_pid = child.getpid();
        // ++++ temporarily access child PCB exclusively
        let exit_code = child.inner_exclusive_access().exit_code;
        // ++++ release child PCB
        *translated_refmut(inner.memory_set.token(), exit_code_ptr) = exit_code;
        return found_pid as isize;
    } else {
        return -2;
    }
    // ---- release current PCB lock automatically
}

pub fn sys_list_apps() -> isize {
    let path = current_task().unwrap().get_working_dir();
    list_files(path.as_str());
    return 0;
}

pub fn sys_shutdown() -> isize {
    shutdown();
    return 0;
}

/// 根据Linux手册
/// The  kill()  system  call  can  be  used to send any signal to any process group or process.
/// 虽然这个函数名字完全和其行为完全没有什么关系, 就是用来给任意进程发送信号
pub fn sys_kill(pid: usize, signum: i32) -> isize {
    if let Some(task) = pid2task(pid) {
        //通过pid找到进程控制块
        if let Some(flag) = SignalFlags::from_bits(1 << signum) {
            // insert the signal if legal
            let mut task_ref = task.inner_exclusive_access();
            if task_ref.signals.contains(flag) {
                //已经有此信号了, do nothing
                return -1;
            }
            task_ref.signals.insert(flag); //在未处理信号集合中插入此信号
            println!(
                "\x1b[38;5;208m[SYSCALL : kill] 给程序Pid [{}] 发送信号 Signum [{}] Signals in Process [{:?}]  \x1b[0m",
                pid, signum, task_ref.signals
            );
            return 0;
        } else {
            return -1;
        }
    } else {
        return -1;
    }
}

/// 把该进程的信号掩码设置到PCB数据结构里面去
pub fn sys_sigprocmask(mask: u32) -> isize {
    if let Some(task) = current_task() {
        let mut inner = task.inner_exclusive_access();
        let old_mask = inner.signal_mask;
        if let Some(flag) = SignalFlags::from_bits(mask) {
            //u32类型 转为 SignalFlags 类似枚举类型.
            inner.signal_mask = flag;
            old_mask.bits() as isize
        } else {
            -1
        }
    } else {
        -1
    }
}

pub fn sys_sigreturn() -> isize {
    if let Some(task) = current_task() {
        let mut inner = task.inner_exclusive_access();
        inner.handling_sig = -1;
        // restore the trap context
        let trap_ctx = inner.get_trap_cx();
        *trap_ctx = inner.trap_ctx_backup.unwrap();
        println!("\x1b[38;5;208m[SYSCALL : SIGRETURN] 程序 [{}]  执行SYS_SIGRETURN系统调用 \x1b[0m",task.getpid());
        // Here we return the value of a0 in the trap_ctx,
        // otherwise it will be overwritten after we trap
        // back to the original execution of the application.
        trap_ctx.x[10] as isize
    } else {
        return -1;
    }
}

/// 检查sigaction的参数是否有错误 (有错误返回true).
/// 如果action 或者 old_action为空指针视为错误
/// 如果信号类型为SIGKILL或者SIGSTOP, 只能由内核处理. 不可交给应用程序处理.
fn check_sigaction_error(signal: SignalFlags, action: usize, old_action: usize) -> bool {
    if action == 0
        || old_action == 0
        || signal == SignalFlags::SIGKILL
        || signal == SignalFlags::SIGSTOP
    {
        return true;
    } else {
        return false;
    }
}

pub fn sys_sigaction(
    signum: i32,
    action: *const SignalAction,
    old_action: *mut SignalAction,
) -> isize {
    let token = current_user_token();
    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    if signum as usize > MAX_SIG {
        return -1;
    }
    if let Some(flag) = SignalFlags::from_bits(1 << signum) {
        // 检查入参是否正确
        if check_sigaction_error(flag, action as usize, old_action as usize) {
            return -1;
        }
        let prev_action = inner.signal_actions.table[signum as usize];
        *translated_refmut(token, old_action) = prev_action; //prev_action函数指针 赋值给old_action. 由于是跨虚拟内存空间操作, 需要用translated_refmut
        inner.signal_actions.table[signum as usize] = *translated_ref(token, action); //这也是跨虚拟内存, 本质就是把action函数指针赋值到PCB的信号对应callback函数表.
        println!("\x1b[38;5;208m[SYSCALL : sigaction] 程序 [{}]  信号 [{}] mapping 回调函数 [{:?}]   \x1b[0m",task.getpid(),signum,action);
        return 0;
    } else {
        return -1;
    }
}
