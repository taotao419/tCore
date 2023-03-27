#![no_std]
#![no_main]
#![allow(clippy::println_empty_string)]

use alloc::string::String;
use alloc::vec::Vec;
use user_lib::console::getchar;
use user_lib::{chdir, close, dup, exec, fork, open,pipe, waitpid, OpenFlags};

extern crate alloc;

#[macro_use]
extern crate user_lib;

const LF: u8 = 0x0au8; //回车
const CR: u8 = 0x0du8; //回车
const DL: u8 = 0x7fu8; //DELETE
const BS: u8 = 0x08u8; //Back Space

#[derive(Debug)]
struct ProcessArguments {
    input: String,             //重定向 之 输入文件
    output: String,            //重定向 之 输出文件
    args_copy: Vec<String>,    //用空格切开的参数字符串集合
    args_addr: Vec<*const u8>, //参数集合中 每个参数字符串头的指针
}

impl ProcessArguments {
    pub fn new(command: &str) -> Self {
        let args: Vec<_> = command.split(' ').collect();
        let mut args_copy: Vec<String> = args
            .iter()
            .filter(|&arg| !arg.is_empty())
            .map(|&arg| {
                let mut string = String::new();
                string.push_str(arg);
                string.push('\0');
                return string;
            })
            .collect();

        //redirect input
        let mut input = String::new();
        if let Some((idx, _)) = args_copy
            .iter()
            .enumerate()
            .find(|(_, arg)| arg.as_str() == "<\0")
        {
            input = args_copy[idx + 1].clone(); // idx是"<"的位置, 所以idx+1 会带上后面的\0
                                                // drain(idx..=idx+1) 意思就是 remove args_copy[idx]这里元素
            args_copy.drain(idx..=idx + 1); //args_copy去除这个 "argXXX <\0" 输入重定向的字符串
        }

        //redirect output
        let mut output = String::new();
        if let Some((idx, _)) = args_copy
            .iter()
            .enumerate()
            .find(|(_, arg)| arg.as_str() == ">\0")
        {
            output = args_copy[idx + 1].clone();
            args_copy.drain(idx..=idx + 1);
        }

        let mut args_addr: Vec<*const u8> = args_copy.iter().map(|arg| arg.as_ptr()).collect();
        args_addr.push(core::ptr::null::<u8>());

        return Self {
            input,
            output,
            args_copy,
            args_addr,
        };
    }
}

#[no_mangle]
pub fn main() -> i32 {
    println!("Rust user shell");
    let mut line: String = String::new();
    let mut current_working_dir: String = String::from("/");
    print!("/ >>");
    loop {
        let c = getchar();
        match c {
            LF | CR => {
                //判断回车之后
                println!("");
                if line.starts_with("cd ") {
                    line.push('\0');
                    println!("change dir");
                    let path = &line[3..line.len()];

                    chdir(path);
                    current_working_dir = String::from(path);
                    print!("{} >>", path);
                    line.clear(); //回车意味着提示符清空
                    continue;
                }
                if !line.is_empty() {
                    let splitted: Vec<_> = line.as_str().split('|').collect(); // 以'|'分割, 分割后就是2个带参数的process
                    let process_arguments_list: Vec<_> = splitted
                        .iter()
                        .map(|&cmd| ProcessArguments::new(cmd))
                        .collect();
                    let mut valid = true;
                    for (i, process_args) in process_arguments_list.iter().enumerate() {
                        if i == 0 {
                            if !process_args.output.is_empty() {
                                valid = false; //第一个参数如果是重定向之输出文件, 命令格式报错
                            } 
                        }else if i == process_arguments_list.len() - 1 {
                                if !process_args.input.is_empty() {
                                    valid = false; //最后一个参数如果是重定向值输入文件, 命令格式报错
                                }
                        } else if !process_args.output.is_empty()
                                || !process_args.input.is_empty()
                        {
                                valid = false; //同一个参数里不可能既有重定向输入文件 又有重定向输出文件. 如果这样必然报错
                        }
                    }
                    if process_arguments_list.len() == 1 {
                        valid = true; //如果只有一个参数对象, 上面的约束条件就无视. 我感觉是为了单元测试用的
                    }
                    if !valid {
                        println!("Invalid command : Inputs/Outputs cannot be correctly binded!");
                    } else {
                        // create pipe
                        let mut pipes_fd: Vec<[usize; 2]> = Vec::new();
                        if !process_arguments_list.is_empty() {
                            for _ in 0..process_arguments_list.len() - 1 {
                                let mut pipe_fd = [0usize; 2];
                                pipe(&mut pipe_fd);
                                pipes_fd.push(pipe_fd);//一个命令行可以拆出多个管道
                            }
                        }
                        let mut children: Vec<_> = Vec::new();
                        for (i, process_argument) in process_arguments_list.iter().enumerate() {
                            let pid = fork();
                            if pid == 0 {
                                //fork出 子进程开始
                                let input = &process_argument.input;
                                let output = &process_argument.output;
                                let args_copy = &process_argument.args_copy;
                                let args_addr = &process_argument.args_addr;
                                // redirect input
                                // ex : wc < file1.txt
                                if !input.is_empty() {
                                    let input_fd = open(input.as_str(), OpenFlags::RDONLY);
                                    if input_fd == -1 {
                                        println!("Error when opening file {}", input);
                                        return -4;
                                    }
                                    let input_fd = input_fd as usize;
                                    close(0); // 先关闭STD_IN
                                    assert_eq!(dup(input_fd), 0); //用 file1.txt替换原有的STD_IN
                                    close(input_fd); // 因为应用进程的后续执行不会用到输入文件原来的描述符input_fd, 所以就将其关掉.
                                }
                                // redirect output
                                // ex : echo 'hello world' > file2.txt
                                if !output.is_empty() {
                                    let output_fd = open(
                                        output.as_str(),
                                        OpenFlags::CREATE | OpenFlags::WRONLY,
                                    );
                                    if output_fd == -1 {
                                        println!("Error when opening file {}", output);
                                        return -4;
                                    }
                                    let output_fd = output_fd as usize;
                                    close(1); // 先关闭STD_OUT 
                                    assert_eq!(dup(output_fd), 1); //用file2.txt 替换原有的STD_OUT. 确定复制出的output_fd的描述符还是1
                                    close(output_fd);
                                }
                                // receive input from the previous process
                                // TODO : 理解的有点问题, 重新理解
                                if i > 0 {
                                    close(0); //把本进程的文件描述符fd=0 **标准输入** 关闭
                                    //pipes_fd.get(i - 1) 意味着拿上个进程的读口
                                    let read_end = pipes_fd.get(i - 1).unwrap()[0]; //N个带参数的进程, 意味着有N-1个管道
                                    //把上个进程的读口 重定向到本进程的**标准输入**
                                    assert_eq!(dup(read_end), 0);
                                }
                                // send output to the next process
                                // TODO : 理解的有点问题, 重新理解
                                if i < process_arguments_list.len() - 1 {
                                    close(1); //把本进程的文件描述符fd=1 **标准输出** 关闭
                                    let write_end = pipes_fd.get(i).unwrap()[1];
                                    //把本进程的**标准输出** 重定向到下个进程的**标准输出**
                                    assert_eq!(dup(write_end), 1);
                                }
                                // close all pipe ends inherited from the parent process
                                for pipe_fd in pipes_fd.iter(){
                                    // 之前的fd 全部都复制了, 原来的可以关闭了
                                    close(pipe_fd[0]);
                                    close(pipe_fd[1]);
                                }
                                // execute new application
                                // 根据命令行 要打开多个带参数新进程
                                if exec(args_copy[0].as_str(),args_addr.as_slice())==-1{
                                    println!("Error when executing!");
                                    return -4;
                                }
                                unreachable!();
                            } else {
                                //父进程执行这里逻辑
                                children.push(pid);
                            }
                        }
                        for pipe_fd in pipes_fd.iter(){
                            close(pipe_fd[0]);
                            close(pipe_fd[1]);
                        }
                        let mut exit_code:i32=0;
                        for pid in children.into_iter(){
                            //这个是loop的wait, 很有可能子进程exec不成功, 它的进程需要父进程立即回收掉.
                            let exit_pid = waitpid(pid as usize, &mut exit_code); 
                            assert_eq!(pid, exit_pid);
                            println!("Shell: Process {} exited with code {}", pid, exit_code);
                        }
                    }
                    line.clear(); //回车意味着提示符清空
                }
                print!("{} >>", current_working_dir);
            }
            BS | DL => {
                //删除 退格键
                if !line.is_empty() {
                    print!("{}", BS as char); //why twice?
                    print!(" ");
                    print!("{}", BS as char);
                    line.pop();
                }
            }
            _ => {
                //默认情况就是提示符打印输入的字符
                print!("{}", c as char);
                line.push(c as char);
            }
        }
    }
}
