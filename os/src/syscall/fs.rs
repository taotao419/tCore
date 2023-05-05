//! File and filesystem-related syscalls

use crate::fs::{make_pipe, open_file, OpenFlags};
use crate::mm::{translated_byte_buffer, translated_refmut, translated_str, UserBuffer};
use crate::task::{current_process,current_task, current_user_token, suspend_current_and_run_next};
use alloc::sync::Arc;

/// write buf of length `len`  to a file with `fd`
pub fn sys_write(fd: usize, buf: *const u8, len: usize) -> isize {
    let token = current_user_token();
    let process = current_process();
    let inner = process.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        if !file.writable() {
            return -1;
        }
        let file = file.clone();
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        file.write(UserBuffer::new(translated_byte_buffer(token, buf, len))) as isize
    } else {
        return -1;
    }
}

pub fn sys_read(fd: usize, buf: *const u8, len: usize) -> isize {
    let token = current_user_token();
    let process = current_process();
    let inner = process.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        let file = file.clone();
        if !file.readable() {
            return -1;
        }
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        file.read(UserBuffer::new(translated_byte_buffer(token, buf, len))) as isize
    } else {
        return -1;
    }
}

pub fn sys_open(path: *const u8, flags: u32) -> isize {
    let process = current_process();
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(inode) = open_file(path.as_str(), OpenFlags::from_bits(flags).unwrap()) {
        let mut inner = process.inner_exclusive_access();
        let fd = inner.alloc_fd();
        inner.fd_table[fd] = Some(inode);
        fd as isize
    } else {
        return -1;
    }
}

pub fn sys_close(fd: usize) -> isize {
    let process = current_process();
    let mut inner = process.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if inner.fd_table[fd].is_none() {
        return -1;
    }
    inner.fd_table[fd].take();
    return 0;
}

pub fn sys_pipe(pipe: *mut usize) -> isize {
    let process = current_process();
    let token = current_user_token();
    let mut inner = process.inner_exclusive_access();
    let (pipe_read, pipe_write) = make_pipe();
    let read_fd = inner.alloc_fd();
    inner.fd_table[read_fd] = Some(pipe_read);
    let write_fd = inner.alloc_fd();
    inner.fd_table[write_fd] = Some(pipe_write);
    //这里pipe 同时也是user app的一个指针, 指向的一个pipe数组. 由于内核层与用户层分属两套不同的内存地址空间.
    //需要先拿到用户层app的token , 内核层才能翻译出用户层这个指针具体在哪里.
    //本质上下面两行代码就是
    // pipe[0] = read_fd;
    // pipe[1] = write_fd;
    println!("\x1b[38;5;208m[SYSCALL : pipe] Create Pipe, User Pipe Point [{:?}] Read fd [{}] and Write fd [{}]  \x1b[0m",pipe, read_fd, write_fd);
    *translated_refmut(token, pipe) = read_fd;
    *translated_refmut(token, unsafe { pipe.add(1) }) = write_fd;
    return 0;
}

pub fn sys_dup(fd: usize) -> isize {
    let process = current_process();
    let mut inner = process.inner_exclusive_access();
    // fd大于文件描述符表的长度 肯定是错的
    if fd >= inner.fd_table.len() {
        return -1;
    }
    // 文件描述符表里fd索引的格子为空 肯定也是错的
    if inner.fd_table[fd].is_none() {
        return -1;
    }
    let new_fd = inner.alloc_fd(); // 申请一个新的文件描述符
    inner.fd_table[new_fd] = Some(Arc::clone(inner.fd_table[fd].as_ref().unwrap())); // 新的文件描述符指向的还是老的文件描述符指向的文件
    println!("\x1b[38;5;208m[SYSCALL : dup pipe] clone fd [{}] ==> new fd [{}]  \x1b[0m",fd, new_fd);
    return new_fd as isize;
}
