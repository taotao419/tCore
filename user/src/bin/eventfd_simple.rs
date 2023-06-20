#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;

use user_lib::{close, eventfd, read, write, EventfdFlags};

#[no_mangle]
pub fn main() -> i32 {
    let num = 10;
    let fd = eventfd(num, EventfdFlags::DEFAULT);
    println!("eventfd file descriptor {}", fd);
    if fd == -1 {
        panic!("Error occured when creating eventfd");
    }
    let fd = fd as usize;
    let mut buf = [0u8; 4];
    let size = read(fd, &mut buf) as usize;
    println!("[USER] read eventfd size [{}]", size);
    
    let bytes: [u8; 4] = num.to_be_bytes(); 
    write(fd, &buf);
    println!("[USER] write to eventfd [{}]", num);

    let mut buf1 = [0u8; 4];
    read(fd, &mut buf1) as usize;
    let num2: u32 = u32::from_be_bytes(buf1);
    println!("[USER] read from eventfd [{}] again", num2);

    close(fd);
    0
}
