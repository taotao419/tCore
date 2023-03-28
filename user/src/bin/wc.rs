#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;

use user_lib::read;

#[no_mangle]
pub fn main(_argc: usize, _argv: &[&str]) -> i32 {
    let mut buf = [0u8; 256];
    let mut lines = 0usize; //STD_IN 文件行数
    let mut total_size = 0usize; //STD_IN 文件大小
    loop {
        let len = read(0, &mut buf) as usize;
        if len == 0 {
            break;
        }
        total_size += len;
        let string = core::str::from_utf8(&buf[..len]).unwrap();
        lines += string.chars().fold(0, |acc, c| acc + if c == '\n' { 1 } else { 0 });
    }
    if total_size>0{
        lines+=1; //如果不是空文件, 起码得有一行
    }
    println!("{}",lines);
    return 0;
}
