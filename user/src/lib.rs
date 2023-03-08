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

use buddy_system_allocator::LockedHeap;
use syscall::*;

const USER_HEAP_SIZE: usize = 32768;

static mut HEAP_SPACE: [u8; USER_HEAP_SIZE] = [0; USER_HEAP_SIZE];

#[global_allocator]
static HEAP: LockedHeap = LockedHeap::empty();

#[alloc_error_handler]
pub fn handle_alloc_error(layout: core::alloc::Layout) -> ! {
    panic!("Heap allocation error, layout = {:?}", layout);
}

///这一块是很trick的地方, 在汇编入口.text.entry 也就是汇编代码的第一行 开始执行如下方法
/// http://rcore-os.cn/rCore-Tutorial-Book-v3/chapter2/2application.html#id4
#[no_mangle]
#[link_section = ".text.entry"]
pub extern "C" fn _start() -> ! {
    unsafe {
        HEAP.lock()
            .init(HEAP_SPACE.as_ptr() as usize, USER_HEAP_SIZE);
    }
    println!("start from main");
    exit(main());
}

#[linkage = "weak"]
#[no_mangle]
fn main() -> i32 {
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

pub fn open(path: &str, flags: OpenFlags) -> isize {
    return sys_open(path,flags.bits);
}

pub fn close(fd:usize)->isize{
    return sys_close(fd);
}

pub fn read(fd: usize, buf: &mut [u8]) -> isize {
    sys_read(fd, buf)
}

pub fn write(fd: usize, buf: &[u8]) -> isize {
    return sys_write(fd, buf);
}

pub fn exit(exit_code: i32) -> !{
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

pub fn chdir(path: &str) ->isize {
    return sys_chdir(path)
}

pub fn fork() -> isize {
    sys_fork()
}

pub fn exec(path: &str) -> isize {
    sys_exec(path)
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
pub fn sleep(period_ms: usize) {
    let start = sys_get_time();
    while sys_get_time() < start + period_ms as isize {
        sys_yield();
    }
}

pub fn shutdown() {
    sys_shutdown();
}

pub fn list_apps() {
    sys_list_apps();
}
