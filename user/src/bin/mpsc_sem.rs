#![no_std]
#![no_main]

use alloc::vec::Vec;
use user_lib::{exit, semaphore_create, semaphore_down, semaphore_up, thread_create, waittid};

#[macro_use]
extern crate user_lib;

extern crate alloc;

const SEM_MUTEX: usize = 0;
const SEM_PRODUCE: usize = 1;
const SEM_CONSUME: usize = 2;
const BUFFER_SIZE: usize = 8;
static mut BUFFER: [usize; BUFFER_SIZE] = [0; BUFFER_SIZE];
static mut FRONT: usize = 0;
static mut TAIL: usize = 0;
const PRODUCER_COUNT: usize = 4;
const NUMBER_PER_PRODUCER: usize = 100;

unsafe fn producer(id: *const usize) -> ! {
    let id = *id;
    for _ in 0..NUMBER_PER_PRODUCER {
        semaphore_down(SEM_PRODUCE); //生产许可数 减一
        semaphore_down(SEM_MUTEX); //上锁
        BUFFER[TAIL] = id;
        TAIL = (TAIL + 1) % BUFFER_SIZE;
        semaphore_up(SEM_MUTEX); //解锁
        semaphore_up(SEM_CONSUME); //消费许可数 加一
    }
    exit(0)
}

unsafe fn comsumer() -> ! {
    for _ in 0..PRODUCER_COUNT * NUMBER_PER_PRODUCER {
        semaphore_down(SEM_CONSUME); //消费许可数 减一
        semaphore_down(SEM_MUTEX); // LOCK
        print!("{} ", BUFFER[FRONT]);
        FRONT = (FRONT + 1) % BUFFER_SIZE;
        semaphore_up(SEM_MUTEX); // UNLOCK
        semaphore_up(SEM_PRODUCE); // 生产许可数 加一
    }
    println!("");
    exit(0)
}

#[no_mangle]
pub fn main() -> i32 {
    assert_eq!(semaphore_create(1) as usize, SEM_MUTEX);
    assert_eq!(semaphore_create(BUFFER_SIZE) as usize, SEM_PRODUCE);
    assert_eq!(semaphore_create(0) as usize, SEM_CONSUME);

    let ids: Vec<_> = (0..PRODUCER_COUNT).collect();
    let mut threads = Vec::new();
    for i in 0..PRODUCER_COUNT {
        threads.push(thread_create(
            producer as usize,
            &ids.as_slice()[i] as *const _ as usize,
        ));
    }
    threads.push(thread_create(consumer as usize, 0));
    // wait for all threads to complete 也就是主线程等其他线程 即join
    for thread in threads.iter(){
        waittid(*thread as usize);
    }
    println!("mpsc_sem passed!");
    return 0;
}
