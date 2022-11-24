#![no_main]
#![no_std]
#![feature(panic_info_message)]
use core::arch::global_asm;

#[macro_use]
mod console;
mod lang_items;
mod sbi;

global_asm!(include_str!("entry.asm"));

#[no_mangle]
pub fn rust_main()->!{
    clear_bss();
    println!("Hello world!");
    println!("你好世界");
    panic!("Shutdown machine!");
}

/// clear BSS segment
pub fn clear_bss() {
    extern "C" {
        fn sbss();
        fn ebss();
    }
    (sbss as usize..ebss as usize).for_each(|a| 
        unsafe { (a as *mut u8).write_volatile(0) 
    });
}