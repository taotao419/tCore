#![no_std]
#![feature(linkage)]
#![feature(panic_info_message)]
#![feature(alloc_error_handler)]

#[macro_use]
pub mod console;
mod lang_items;
mod syscall;

extern crate alloc;
#[macro_use]
extern crate bitflags;

use alloc::vec::Vec;
use buddy_system_allocator::LockedHeap;
use syscall::*;

const USER_HEAP_SIZE: usize = 32768;
const LOG_FLAG: bool = true;

static mut HEAP_SPACE: [u8; USER_HEAP_SIZE] = [0; USER_HEAP_SIZE];

#[global_allocator]
static HEAP: LockedHeap = LockedHeap::empty();

#[alloc_error_handler]
pub fn handle_alloc_error(layout: core::alloc::Layout) -> ! {
    panic!("Heap allocation error, layout = {:?}", layout);
}

///这一块是很trick的地方, 在汇编入口.text.entry 也就是汇编代码的第一行 开始执行如下方法
/// http://rcore-os.cn/rCore-Tutorial-Book-v3/chapter2/2application.html#id4
/// 在task.rs#exec方法里面 往寄存器a0/a1存入的值, 会在这里作为入参argc与argv.
/// argc : 命令行入参个数
/// argv : 命令行字符串数组的base地址
#[no_mangle]
#[link_section = ".text.entry"]
pub extern "C" fn _start(argc: usize, argv: usize) -> ! {
    unsafe {
        HEAP.lock()
            .init(HEAP_SPACE.as_ptr() as usize, USER_HEAP_SIZE);
    }

    let mut v: Vec<&'static str> = Vec::new();
    for i in 0..argc {
        // 读取argv[x] 这个usize指针, 它会指向对应的参数字符串起始位置
        let str_start =
            unsafe { ((argv + i * core::mem::size_of::<usize>()) as *const usize).read_volatile() };
        // 一路读到字符串结尾 即\0, 知道字符串长度
        let len = (0usize..)
            .find(|i| unsafe { ((str_start + *i) as *const u8).read_volatile() == 0 })
            .unwrap();
        // 知道字符串开头地址, 和字符串长度. 就可以把这个字符串取出来放到vec里
        let arg_str = core::str::from_utf8(unsafe {
            core::slice::from_raw_parts(str_start as *const u8, len)
        })
        .unwrap();
        v.push(arg_str);
        println!(
            "\x1b[32m[_start]  read argv index [{}] str_start:[{}] len:[{}] arg_str:[{}] \x1b[0m",
            i, str_start, len, arg_str
        );
    }

    println!("start from main");
    exit(main(argc, v.as_slice()));
}

#[linkage = "weak"]
#[no_mangle]
fn main(_argc: usize, _argv: &[&str]) -> i32 {
    panic!("Cannot find main!");
}

bitflags! {
    pub struct OpenFlags:u32{
        const RDONLY=0;
        const WRONLY=1<<0; //第0位 设置为1
        const RDWR=1<<1; //第1位 设置为1 可读可写
        const CREATE=1<<9; //第9位 设置为1 创建文件
        const TRUNC=1<<10; //第10位 设置为1 清空文件内容 并将该文件的大小归零
    }
}

pub fn dup(fd: usize) -> isize {
    sys_dup(fd)
}

pub fn open(path: &str, flags: OpenFlags) -> isize {
    return sys_open(path, flags.bits);
}

pub fn close(fd: usize) -> isize {
    return sys_close(fd);
}

pub fn pipe(pipe_fd: &mut [usize]) -> isize {
    return sys_pipe(pipe_fd);
}

pub fn read(fd: usize, buf: &mut [u8]) -> isize {
    sys_read(fd, buf)
}

pub fn write(fd: usize, buf: &[u8]) -> isize {
    return sys_write(fd, buf);
}

pub fn exit(exit_code: i32) -> ! {
    println!(
        "\x1b[93m [USER] this is call exit from user lib -- pid : [{}] -- exit_code : [{}] \x1b[0m",
        getpid(),
        exit_code
    );
    sys_exit(exit_code);
}

pub fn yield_() -> isize {
    return sys_yield();
}

pub fn get_time() -> isize {
    return sys_get_time();
}

pub fn getpid() -> isize {
    sys_getpid()
}

pub fn get_cwd() -> isize {
    sys_get_cwd()
}

pub fn chdir(path: &str) -> isize {
    return sys_chdir(path);
}

pub fn fork() -> isize {
    sys_fork()
}

pub fn exec(path: &str, args: &[*const u8]) -> isize {
    sys_exec(path, args)
}

///父进程等待任一一个子进程销毁
pub fn wait(exit_code: &mut i32) -> isize {
    loop {
        //-1表示任意一个子进程
        match sys_waitpid(-1, exit_code as *mut _) {
            -2 => {
                //如果返回值是 -2 , 则需要让出CPU, 进入下一轮循环
                yield_();
            }
            //a real pid 此子进程销毁
            exit_pid => {
                println!("user app call sys_wait, exit pid : [{}]", exit_pid);
                return exit_pid;
            }
        }
    }
}

pub fn waitpid(pid: usize, exit_code: &mut i32) -> isize {
    loop {
        match sys_waitpid(pid as isize, exit_code as *mut _) {
            -2 => {
                //如果返回值是 -2 , 则需要让出CPU, 进入下一轮循环
                yield_();
            }
            //-1 表示没有子进程 or a real pid 此子进程销毁
            exit_pid => return exit_pid,
        }
    }
}
pub fn sleep(sleep_ms: usize) {
    sys_sleep(sleep_ms);
}

pub fn thread_create(entry: usize, arg: usize) -> isize {
    sys_thread_create(entry, arg)
}

pub fn gettid() -> isize {
    sys_gettid()
}

pub fn waittid(tid: usize) -> isize {
    loop {
        match sys_waittid(tid) {
            -2 => {
                yield_();
            }
            exit_code => return exit_code,
        }
    }
}

pub fn shutdown() {
    sys_shutdown();
}

pub fn list_apps() {
    sys_list_apps();
}

#[repr(C, align(16))]
#[derive(Debug, Clone, Copy)]
pub struct SignalAction {
    pub handler: usize,
    pub mask: SignalFlags,
}

impl Default for SignalAction {
    fn default() -> Self {
        Self {
            handler: 0,
            mask: SignalFlags::empty(),
        }
    }
}

pub const SIGDEF: i32 = 0; // Default signal handling
pub const SIGHUP: i32 = 1;
pub const SIGINT: i32 = 2;
pub const SIGQUIT: i32 = 3;
pub const SIGILL: i32 = 4;
pub const SIGTRAP: i32 = 5;
pub const SIGABRT: i32 = 6;
pub const SIGBUS: i32 = 7;
pub const SIGFPE: i32 = 8;
pub const SIGKILL: i32 = 9;
pub const SIGUSR1: i32 = 10;
pub const SIGSEGV: i32 = 11;
pub const SIGUSR2: i32 = 12;
pub const SIGPIPE: i32 = 13;
pub const SIGALRM: i32 = 14;
pub const SIGTERM: i32 = 15;
pub const SIGSTKFLT: i32 = 16;
pub const SIGCHLD: i32 = 17;
pub const SIGCONT: i32 = 18;
pub const SIGSTOP: i32 = 19;
pub const SIGTSTP: i32 = 20;
pub const SIGTTIN: i32 = 21;
pub const SIGTTOU: i32 = 22;
pub const SIGURG: i32 = 23;
pub const SIGXCPU: i32 = 24;
pub const SIGXFSZ: i32 = 25;
pub const SIGVTALRM: i32 = 26;
pub const SIGPROF: i32 = 27;
pub const SIGWINCH: i32 = 28;
pub const SIGIO: i32 = 29;
pub const SIGPWR: i32 = 30;
pub const SIGSYS: i32 = 31;

bitflags! {
    pub struct SignalFlags: i32 {
        const SIGDEF = 1; // Default signal handling
        const SIGHUP = 1 << 1;
        const SIGINT = 1 << 2;
        const SIGQUIT = 1 << 3;
        const SIGILL = 1 << 4;
        const SIGTRAP = 1 << 5;
        const SIGABRT = 1 << 6;
        const SIGBUS = 1 << 7;
        const SIGFPE = 1 << 8;
        const SIGKILL = 1 << 9;
        const SIGUSR1 = 1 << 10;
        const SIGSEGV = 1 << 11;
        const SIGUSR2 = 1 << 12;
        const SIGPIPE = 1 << 13;
        const SIGALRM = 1 << 14;
        const SIGTERM = 1 << 15;
        const SIGSTKFLT = 1 << 16;
        const SIGCHLD = 1 << 17;
        const SIGCONT = 1 << 18;
        const SIGSTOP = 1 << 19;
        const SIGTSTP = 1 << 20;
        const SIGTTIN = 1 << 21;
        const SIGTTOU = 1 << 22;
        const SIGURG = 1 << 23;
        const SIGXCPU = 1 << 24;
        const SIGXFSZ = 1 << 25;
        const SIGVTALRM = 1 << 26;
        const SIGPROF = 1 << 27;
        const SIGWINCH = 1 << 28;
        const SIGIO = 1 << 29;
        const SIGPWR = 1 << 30;
        const SIGSYS = 1 << 31;
    }
}

pub fn kill(pid: usize, signum: i32) -> isize {
    sys_kill(pid, signum)
}

pub fn sigaction(
    signum: i32,
    action: Option<&SignalAction>,
    old_action: Option<&mut SignalAction>,
) -> isize {
    sys_sigaction(
        signum,
        action.map_or(core::ptr::null(), |a| a), //如果是Option None就传入空指针Null, 有非空指针就传指针
        old_action.map_or(core::ptr::null_mut(), |a| a), //同上
    )
}

pub fn sigprocmask(mask: u32) -> isize {
    sys_sigprocmask(mask)
}

pub fn sigreturn() -> isize {
    sys_sigreturn()
}

pub fn mutex_create() -> isize {
    sys_mutex_create(false)
}

pub fn mutex_blocking_create() -> isize {
    sys_mutex_create(true)
}

pub fn mutex_lock(mutex_id: usize) {
    sys_mutex_lock(mutex_id);
}

pub fn mutex_unlock(mutex_id: usize) {
    sys_mutex_unlock(mutex_id);
}

pub fn semaphore_create(res_count: usize) -> isize {
    sys_semaphore_create(res_count)
}

pub fn semaphore_up(sem_id: usize) {
    sys_semaphore_up(sem_id);
}

pub fn semaphore_down(sem_id: usize) {
    sys_semaphore_down(sem_id);
}

pub fn condvar_create() -> isize {
    return sys_condvar_create();
}

pub fn condvar_signal(condvar_id: usize) {
    sys_condvar_signal(condvar_id);
}

pub fn condvar_wait(condvar_id: usize, mutex_id: usize) {
    sys_condvar_wait(condvar_id, mutex_id);
}

pub fn condvar_signal_all(condvar_id: usize) {
    sys_condvar_signal_all(condvar_id);
}

#[macro_export]
macro_rules! vload {
    ($var_ref: expr) => {
        unsafe { core::intrinsics::volatile_load($var_ref as *const _ as _) }
    };
}
