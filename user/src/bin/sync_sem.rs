#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;

extern crate alloc;

use alloc::vec;
use user_lib::{exit, semaphore_create, semaphore_up, sleep, thread_create, waittid, println, semaphore_down};

const SEM_SYNC: usize = 0;

unsafe fn first() -> ! {
    sleep(10);
    println!("First work and wakeup Second");
    semaphore_up(SEM_SYNC);
    exit(0)
}

unsafe fn second() -> ! {
    println!("Second want to continue, but need to wait first");
    semaphore_down(SEM_SYNC);
    println!("Second can work now");
    exit(0)
}

#[no_mangle]
pub fn main() -> i32 {
    assert_eq!(semaphore_create(0) as usize, SEM_SYNC);

    let threads = vec![
        thread_create(first as usize, 0),
        thread_create(second as usize, 0),
    ];

    for thread in threads.iter(){
        waittid(*thread as usize);
    }
    println!("sync_sem passed!");
    return 0;
}
