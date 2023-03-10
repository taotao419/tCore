#![no_std]
#![no_main]

extern crate user_lib;

use user_lib::list_apps;

#[no_mangle]
pub fn main() -> i32 {
    list_apps();
    return 0;
}
