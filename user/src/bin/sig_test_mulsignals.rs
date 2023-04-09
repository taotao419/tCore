#![no_std]
#![no_main]

extern crate user_lib;

use user_lib::*;

fn callback1() {
    sleep(500);
    println!("callback 1 user_sig_test passed");
    sigreturn();
}
fn callback2() {
    sleep(500);
    println!("callback 2 user_sig_test passed");
    sigreturn();
}

#[no_mangle]
pub fn main() -> i32 {
    let mut new = SignalAction::default();
    let mut old = SignalAction::default();
    new.handler = callback1 as usize; //设置回调函数

    let mut new1 = SignalAction::default();
    let mut old1 = SignalAction::default();
    new1.handler = callback2 as usize; //设置回调函数

    println!("signal_simple : sigaction");
    // 相当于注册了SIGUSR1事件的回调函数callback (Ln 8)
    if sigaction(SIGUSR1, Some(&new), Some(&mut old)) < 0 {
        panic!("Sigaction failed!");
    }
    if sigaction(SIGUSR2, Some(&new1), Some(&mut old1)) < 0 {
        panic!("Sigaction failed!");
    }
    println!("signal_simple: kill");
    //在执行kill方法时, 会触发事件SIGUSR1
    if kill(getpid() as usize, SIGUSR1) < 0 {
        println!("kill failed!");
        exit(1);
    }
    if kill(getpid() as usize, SIGUSR2) < 0 {
        println!("kill failed!");
        exit(1);
    }
    println!("signal_simple: Done");
    return 0;
}
