pub fn info<T: core::fmt::Display>(msg: &str, t: T) {
    println!("\x1b[34m[INFO] {} -- {}\x1b[0m", msg, t);
}

pub fn info2<T: core::fmt::Display>(msg: &str, t1: T, t2: T) {
    println!("\x1b[34m[INFO] {} -- {} -- {}\x1b[0m", msg, t1, t2);
}

pub fn warn<T: core::fmt::Display>(msg: &str, t: T) {
    println!("\x1b[93m[WARN] {} -- {}\x1b[0m", msg, t);
}

pub fn error<T: core::fmt::Display>(msg: &str, t: T) {
    println!("\x1b[31m[ERROR] {} -- {}\x1b[0m", msg, t);
}
