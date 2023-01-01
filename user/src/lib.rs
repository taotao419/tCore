#![no_std]
#![feature(linkage)]
#![feature(panic_info_message)]

#[macro_use]
pub mod console;
mod lang_items;
mod syscall;

#[no_mangle]
#[link_section = ".text.entry"]
pub extern "C" fn _start() -> ! {
    exit(main());
    panic!("unreachable after sys_exit!");
}

#[linkage = "weak"]
#[no_mangle]
fn main() -> i32 {
    panic!("Cannot find main!");
}

use syscall::*;

pub fn write(fd: usize, buf: &[u8]) -> isize {
   return sys_write(fd, buf);
}

pub fn exit(exit_code: i32) -> isize {
    return sys_exit(exit_code);
}

pub fn yield_()->isize{
    return sys_yield();
}

pub fn get_time() -> isize{
    return sys_get_time();
}
