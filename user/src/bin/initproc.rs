#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;

use user_lib::{exec, fork, wait, yield_};

#[no_mangle]
fn main() -> i32 {
    if fork() == 0 {
        //此时 fork()之后, 已经分裂为独立的两个进程了, 这部分是子进程开始运行user_shell这个User APP
        exec("user_shell\0",&[core::ptr::null::<u8>()]);
    } else {
        //这里还是父进程 或者说原进程. 在原进程的代码执行中永远会执行else块中的代码
        loop {
            let mut exit_code: i32 = 0;
            let pid = wait(&mut exit_code);
            //等待有没有准备销毁的子进程 作为所有用户进程的爷爷(祖宗) 时时刻刻准备回收子进程的资源
            if pid == -1 {
                //pid 如果是-1 说明没有子进程准备销毁. 那么就交还CPU控制权
                yield_();
                continue;
            }
            //是有子进程销毁的
            println!(
                "[initproc] Released a zombie process, pid={}, exit_code={}",
                pid, exit_code
            );
        }
    }

    return 0;
}
