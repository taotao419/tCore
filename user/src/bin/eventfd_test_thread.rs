#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;
extern crate alloc;

use alloc::vec;
use user_lib::{exit, thread_create, waittid, sleep};
use user_lib::{close, eventfd, read, write, EventfdFlags};

const FD:usize =3;

pub fn thread_a() -> ! {
    let mut buf = [0u8; 4];
    let size = read(FD, &mut buf) as usize;
    let num: u32 = u32::from_be_bytes(buf);
    println!("[Thread A] read from eventfd [{}]", num);
    exit(1)
}

pub fn thread_b() -> ! {
    let mut buf = [0u8; 4];
    let size = read(FD, &mut buf) as usize;
    let num: u32 = u32::from_be_bytes(buf);
    println!("[Thread B] read from eventfd [{}]", num);
    exit(2)
}

#[no_mangle]
pub fn main() -> i32 {
    eventfd(0, EventfdFlags::DEFAULT);
    let v = vec![
        thread_create(thread_a as usize, 0),
        thread_create(thread_b as usize, 0),
    ];

    sleep(1000); // sleep 1000ms
    let num_bytes= 15_i32.to_be_bytes();
    write(FD,&num_bytes);
    sleep(1000); // sleep 1000ms
    write(FD,&num_bytes);

    for tid in v.iter() {
        let exit_code = waittid(*tid as usize);
        println!("thread#{} exited with code {}", tid, exit_code);
    }
    println!("main thread exited.");
    close(FD);
    return 0;
}
