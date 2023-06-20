#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;

use user_lib::{eventfd, read, write, EventfdFlags, sleep};
use user_lib::{fork, getpid, wait};

#[no_mangle]
pub fn main() -> i32 {
    assert_eq!(wait(&mut 0i32), -1);
    println!("sys_wait without child process test passed!");
    println!("parent start, pid = {}!", getpid());

    let fd = eventfd(0, EventfdFlags::DEFAULT);
    if fd == -1 {
        panic!("Error occured when creating eventfd");
    }
    let fd = fd as usize;
    let pid = fork();

    if pid == 0 {
        // child process
        println!("child process write 15 to eventfd");
        let num_bytes= 15_i32.to_be_bytes();
        write(fd,&num_bytes);
        0
    } else {
        // sleep(2000); //sleep 2000 ms
        println!("parent about to read");
        let mut buf = [0u8; 4];
        read(fd as usize, &mut buf);
        let read_result: u32 = u32::from_be_bytes(buf);
        println!("Parent read from eventfd [{}] ", read_result);
        0
    }
}
