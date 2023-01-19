#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;

use user_lib::shutdown;

#[no_mangle]
pub fn main() -> i32 {
    println!("Shutdown command from User APP");
    shutdown();
    return 0;
}
