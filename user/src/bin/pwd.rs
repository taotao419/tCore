#![no_std]
#![no_main]

extern crate user_lib;

use user_lib::get_cwd;

#[no_mangle]
pub fn main() -> i32 {
    get_cwd();
    return 0;
}
