#![no_std]
#![no_main]
#![allow(clippy::println_empty_string)]

use alloc::string::String;
use user_lib::console::getchar;
use user_lib::{exec, fork,waitpid};

extern crate alloc;

#[macro_use]
extern crate user_lib;

const LF: u8 = 0x0au8; //回车
const CR: u8 = 0x0du8; //回车
const DL: u8 = 0x7fu8; //DELETE
const BS: u8 = 0x08u8; //Back Space

#[no_mangle]
pub fn main() -> i32 {
    println!("Rust user shell");
    let mut line: String = String::new();
    print!("输入应用名称 >> ");
    loop {
        let c = getchar();
        match c {
            LF | CR => {
                //判断回车之后
                println!("");
                if !line.is_empty() {
                    line.push('\0');
                    let pid = fork();
                    if pid == 0 {
                        //child process
                        if exec(line.as_str()) == -1 {
                            println!("Error when executing!");
                            return -4;
                        }
                        panic!("Unreachable in rust_main!");
                    } else {
                        //original process
                        let mut exit_code: i32 = 0;
                        //这个是loop的wait, 很有可能子进程exec不成功, 它的进程需要父进程立即回收掉.
                        let exit_pid = waitpid(pid as usize, &mut exit_code);
                        assert_eq!(pid, exit_pid);
                        println!("Shell: Process {} exited with code {}", pid, exit_code);
                    }
                    line.clear();//回车意味着提示符清空
                }
                print!("&>>");
            }
            BS | DL => { //删除 退格键
                if !line.is_empty() {
                    print!("{}", BS as char); //why twice?
                    print!(" ");
                    print!("{}", BS as char);
                    line.pop();
                }
            }
            _ => { //默认情况就是提示符打印输入的字符
                print!("{}", c as char);
                line.push(c as char);
            }
        }
    }
}
