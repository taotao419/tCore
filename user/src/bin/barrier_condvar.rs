#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;
extern crate alloc;

use core::cell::UnsafeCell;
use alloc::vec::Vec;
use lazy_static::*;
use user_lib::{condvar_create, condvar_signal, condvar_wait, mutex_create, mutex_unlock, waittid, exit, thread_create, mutex_lock, condvar_signal_all};

const THREAD_NUM: usize = 5;

struct Barrier {
    mutex_id: usize,
    condvar_id: usize,
    count: UnsafeCell<usize>,
}

impl Barrier {
    pub fn new() -> Self {
        Self {
            mutex_id: mutex_create() as usize,
            condvar_id: condvar_create() as usize,
            count: UnsafeCell::new(0),
        }
    }

    pub fn block(&self) {
        mutex_lock(self.mutex_id);
        let count = self.count.get();
        // 这里 获取 count 是由互斥锁保护的临界区
        unsafe {
            *count = *count + 1;
        }
        if unsafe { *count } == THREAD_NUM {
            condvar_signal_all(self.condvar_id);
        } else {
            condvar_wait(self.condvar_id, self.mutex_id);
            // condvar_signal(self.condvar_id); // 由于只能唤醒一个线程, 这里代码的含义是 如果被唤醒, 该线程还需要唤醒其他线程
            // 所以如果上面的if 是 signal_all 就不需要这句了
        }
        mutex_unlock(self.mutex_id);
    }
}

unsafe impl Sync for Barrier {}

lazy_static! {
    static ref BARRIER_AB: Barrier = Barrier::new();
    static ref BARRIER_BC: Barrier = Barrier::new();
    static ref BARRIER_CD: Barrier = Barrier::new();
    static ref BARRIER_DE: Barrier = Barrier::new();
}

fn thread_fn() {
    for _ in 0..300 {
        print!("a");
    }
    BARRIER_AB.block();
    for _ in 0..300 {
        print!("b");
    }
    BARRIER_BC.block();
    for _ in 0..300 {
        print!("c");
    }
    BARRIER_CD.block();
    for _ in 0..300 {
        print!("d");
    }
    BARRIER_DE.block();
    for _ in 0..300 {
        print!("e");
    }

    exit(0)
}

#[no_mangle]
pub fn main() -> i32 {
    let mut v: Vec<isize> = Vec::new();
    for _ in 0..THREAD_NUM {
        v.push(thread_create(thread_fn as usize, 0));
    }
    for tid in v.into_iter() {
        waittid(tid as usize);
    }
    println!("\nOK!");
    0
}
