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
    info("Hello world !");
    warn("Hello world !");
    error("Hello world !");
    println!("你好世界");
    panic!("Shutdown machine!");
}

/// clear BSS segment
pub fn clear_bss() {
    extern "C" {
        fn stext();
        fn etext();
        fn srodata();
        fn erodata();
        fn sdata();
        fn edata();
        fn sbss();
        fn ebss();
    }
    (sbss as usize..ebss as usize).for_each(|a| 
        unsafe { (a as *mut u8).write_volatile(0) 
    });

    println!("\x1b[34m[INFO] text range : [{:#x},{:#x}) \x1b[0m ",stext as usize, etext as usize);
    println!("\x1b[34m[INFO] rodata range : [{:#x},{:#x}) \x1b[0m ",srodata as usize, erodata as usize);
    println!("\x1b[34m[INFO] data range : [{:#x},{:#x}) \x1b[0m ",sdata as usize, edata as usize);
    println!("\x1b[34m[INFO] bss range : [{:#x},{:#x}) \x1b[0m ",sbss as usize, ebss as usize);
    
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