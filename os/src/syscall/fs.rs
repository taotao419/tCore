//! File and filesystem-related syscalls

use crate::fs::{open_file, OpenFlags};
use crate::mm::{translated_byte_buffer, translated_str, UserBuffer};
use crate::sbi::console_getchar;
use crate::task::{current_user_token, suspend_current_and_run_next, current_task};

const FD_STDIN: usize = 0;
const FD_STDOUT: usize = 1;

/// write buf of length `len`  to a file with `fd`
pub fn sys_write(fd: usize, buf: *const u8, len: usize) -> isize {
    let token=current_user_token();
    let task=current_task().unwrap();
    let inner=task.inner_exclusive_access();
    if fd>=inner.fd_table.len(){
        return -1;
    }
    if let Some(file)=&inner.fd_table[fd]{
        if !file.writable(){
            return -1;
        }
        let file=file.clone();
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        file.write(UserBuffer::new(translated_byte_buffer(token, buf, len))) as isize
    }else{
        return -1;
    }
}

pub fn sys_read(fd: usize, buf: *const u8, len: usize) -> isize {
    let token=current_user_token();
    let task=current_task().unwrap();
    let inner=task.inner_exclusive_access();
    if fd>=inner.fd_table.len(){
        return -1;
    } 
    if let Some(file)=&inner.fd_table[fd]{
        if !file.readable(){
            return -1;
        }
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        file.read(UserBuffer::new(translated_byte_buffer(token, buf, len))) as isize
    }else{
        return -1;
    }

    match fd {
        FD_STDIN => {
            assert_eq!(len, 1, "Only support len =1 in sys_read!");
            let mut c: usize;
            loop {
                c=console_getchar();
                if c==0 {
                    suspend_current_and_run_next();
                    continue;
                }else{
                    break;
                }
            }
            let ch=c as u8;
            let mut buffers=translated_byte_buffer(current_user_token(), buf, len);
            unsafe{
                buffers[0].as_mut_ptr().write_volatile(ch);
            }
            return 1;
        }
        _=>{
            panic!("Unsupported fd in sys_read!");
        }
    }
}

pub fn sys_open(path: *const u8, flags:u32)->isize{
    let task=current_task().unwrap();
    let token=current_user_token();
    let path =translated_str(token, path);
    if let Some(inode)=open_file(path.as_str(),OpenFlags::from_bits(flags).unwrap()){
        let mut inner=task.inner_exclusive_access();
        let fd=inner.alloc_fd();
        inner.fd_table[fd]=Some(inode);
        fd as isize
    }else{
        return -1
    }
}

pub fn sys_close(fd:usize)->isize{
    let task=current_task().unwrap();
    let mut inner=task.inner_exclusive_access();
    if fd>=inner.fd_table.len(){
        return -1;
    }
    if inner.fd_table[fd].is_none(){
        return -1;
    }
    inner.fd_table[fd].take();
    return 0;
}