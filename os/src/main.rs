#![no_main]
#![no_std]
#![feature(panic_info_message)]
use core::arch::global_asm;

#[macro_use]
mod console;
pub mod batch;
mod lang_items;
mod sbi;
mod sync;
pub mod syscall;
pub mod trap;

global_asm!(include_str!("entry.asm"));
global_asm!(include_str!("link_app.S"));

/// clear BSS segment
pub fn clear_bss() {
    extern "C" {
        fn sbss();
        fn ebss();
    }
    unsafe {
        core::slice::from_raw_parts_mut(sbss as usize as *mut u8, ebss as usize - sbss as usize)
            .fill(0);
    }
}

/// the rust entry-point of os
#[no_mangle]
pub fn rust_main() -> ! {
    clear_bss();
    println!("[kernel] Hello, world!");
    trap::init();
    batch::init();
    batch::run_next_app();
}

pub fn info(s:&str) {
    println!("\x1b[34m[INFO] {}\x1b[0m",s); 
}

pub fn warn(s:&str) {
    println!("\x1b[93m[WARN] {}\x1b[0m",s); 
}

pub fn error(s:&str) {
    println!("\x1b[31m[ERROR] {}\x1b[0m",s); 
}