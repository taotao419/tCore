//! The main module and entrypoint
//!
//! Various facilities of the kernels are implemented as submodules. The most
//! important ones are:
//!
//! - [`trap`]: Handles all cases of switching from userspace to the kernel
//! - [`task`]: Task management
//! - [`syscall`]: System call handling and implementation
//! - [`mm`]: Address map using SV39
//! - [`sync`]:Wrap a static data structure inside it so that we are able to access it without any `unsafe`.
//!
//! The operating system also starts in this module. Kernel code starts
//! executing from `entry.asm`, after which [`rust_main()`] is called to
//! initialize various pieces of functionality. (See its source code for
//! details.)
//!
//! We then call [`task::run_tasks()`] and for the first time go to
//! userspace.

// #![deny(missing_docs)]
#![allow(warnings)]
#![no_std]
#![no_main]
#![feature(panic_info_message)]
#![feature(alloc_error_handler)]

extern crate alloc;

#[macro_use]
extern crate bitflags;

#[path = "boards/qemu.rs"]
mod board;

#[macro_use]
mod console;
mod config;
mod drivers;
pub mod fs;
pub mod lang_items;
pub mod mm;
mod sbi;
pub mod sync;
pub mod syscall;
pub mod task;
mod timer;
pub mod trap;

use crate::drivers::GPU_DEVICE;
use crate::drivers::chardev::CharDevice;
use crate::drivers::chardev::UART;
use crate::drivers::input::KEYBOARD_DEVICE;
use crate::drivers::input::MOUSE_DEVICE;

core::arch::global_asm!(include_str!("entry.asm"));

/// clear BSS segment
fn clear_bss() {
    extern "C" {
        fn sbss();
        fn ebss();
    }
    unsafe {
        core::slice::from_raw_parts_mut(sbss as usize as *mut u8, ebss as usize - sbss as usize)
            .fill(0);
    }
}

use lazy_static::*;
use sync::UPIntrFreeCell;

lazy_static! {
    pub static ref DEV_NON_BLOCKING_ACCESS: UPIntrFreeCell<bool> =
        unsafe { UPIntrFreeCell::new(false) };
}

/// the rust entry-point of os
#[no_mangle]
pub fn rust_main() -> ! {
    clear_bss();

    mm::init();
    UART.init();
    println!("KERN: init gpu");
    let _gpu = GPU_DEVICE.as_ref(); // GPU_DEVICE 是lazy_static 必须调用它的任何一个方法, 促使其实例化, 并调用构造函数
    println!("KERN: init keyboard");
    let _keyboard = KEYBOARD_DEVICE.as_ref();
    println!("KERN: init mouse");
    let _mouse = MOUSE_DEVICE.as_ref();
    mm::remap_test();
    trap::init();
    trap::enable_timer_interrupt();
    timer::set_next_trigger();
    board::device_init();

    println!(r"   _____       _         U  ___ u   _____       _         U  ___ u        U  ___ u   ____     ");
    println!(r"  |_   _|  U  / \  u      \/ _ \/  |_   _|  U  / \  u      \/ _ \/         \/ _ \/  / __| u  ");
    println!(r"    | |     \/ _ \/       | | | |    | |     \/ _ \/       | | | |         | | | | <\___ \/   ");
    println!(r"   /| |\    / ___ \   .-,_| |_| |   /| |\    / ___ \   .-,_| |_| |     .-,_| |_| |  u___) |   ");
    println!(r"  u |_|U   /_/   \_\   \_)-\___/   u |_|U   /_/   \_\   \_)-\___/       \_)-\___/   |____/>>  ");
    println!(r"  _// \\_   \\    >>        \\     _// \\_   \\    >>        \\              \\      )(  (__) ");
    println!(r" (__) (__) (__)  (__)      (__)   (__) (__) (__)  (__)      (__)            (__)    (__)      ");
    

    fs::list_files("/");
    task::add_initproc();
    *DEV_NON_BLOCKING_ACCESS.exclusive_access() = true;
    println!("after initproc!");
    //trap::enable_interrupt();
    task::run_tasks();
    panic!("Unreachable in rust_main!");
}