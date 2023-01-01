use core::fmt::Debug;

use crate::timer::get_time_ms;

#[allow(dead_code)]
pub fn info<T: Debug>(msg: &str, t: &T) {
    let timing = get_time_ms();
    println!( "\x1b[34m[{time} ms] [INFO] {} -- {:#?}\x1b[0m", msg, t, time = timing);
}

pub fn info2<T: core::fmt::Display>(msg: &str, t1: T, t2: T) {
    let timing = get_time_ms();
    println!("\x1b[34m[{time} ms] [INFO] {} -- {} -- {}\x1b[0m", msg, t1, t2, time = timing);
}

#[allow(dead_code)]
pub fn warn<T: core::fmt::Display>(msg: &str, t: T) {
    let timing = get_time_ms();
    println!("\x1b[93m[{time} ms] [WARN] {} -- {}\x1b[0m", msg, t, time = timing);
}

#[allow(dead_code)]
pub fn error<T: core::fmt::Display>(msg: &str, t: T) {
    println!("\x1b[31m[ERROR] {} -- {}\x1b[0m", msg, t);
}
