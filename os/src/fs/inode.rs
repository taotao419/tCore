use crate::drivers::BLOCK_DEVICE;
use crate::mm::UserBuffer;
use crate::sync::UPSafeCell;
use alloc::sync::Arc;
use alloc::vec::Vec;
use bitflags::*;

use easy_fs::{EasyFileSystem, Inode};
use lazy_static::*;

/// A wrapper around a filesystem inode
/// to implement File trait atop
pub struct OSInode {
    readable: bool,
    writable: bool,
    inner: UPSafeCell<OSInodeInner>,
}

/// The OS inode inner in 'UPSafeCell'
pub struct OSInodeInner {
    offset: usize,
    inode: Arc<Inode>,
}

impl OSInode {
    /// Construct an OS inode from a inode
    pub fn new(readable: bool, writable: bool, inode: Arc<Inode>) -> Self {
        Self {
            readable,
            writable,
            inner: unsafe { UPSafeCell::new(OSInodeInner { offset: 0, inode }) },
        }
    }
    /// Read all data inside a inode into vector
    pub fn read_all(&self) -> Vec<u8> {
        let mut inner = self.inner.exclusive_access();
        let mut buffer = [0u8; 512];
        let mut v: Vec<u8> = Vec::new();
        loop {
            let len = inner.inode.read_at(inner.offset, &mut buf);
            if len == 0 {
                break;
            }
            inner.offset+=len;
            v.extend_from_slice(&buffer[..len]);
        }
        return v;
    }
}

lazy_static! {
    pub static ref ROOT_INODE: Arc<Inode> = {
        let efs = EasyFileSystem::open(BLOCK_DEVICE.clone());
        return Arc::new(EasyFileSystem::root_inode(&efs));
    };
}

/// List all files in the filesystems
pub fn list_apps() {
    println!("/***** APPS *****/");
    for app in ROOT_INODE.ls() {
        println!("{}", app);
    }
    println!("/***********/");
}

bitflags! {
    /// Open file flags
    pub struct OpenFlags:u32{
        ///Read only
        const RDONLY=0;
        ///Write only
        const WRONLY=1<<0;
        // Read & Write
        const RDWR=1<<1;
        // Allow create
        const CREATE =1<<9;
        // Clear file and return an empty one
        const TRUNC = 1<<10;
    }
}

impl OpenFlags {
    /// Do not check validity for simplicity 简单起见, 假定标志位格式必须合法
    /// Return (readable, writable) 返回结构体 括号里面2个bool值, 标识是否可读, 是否可写
    pub fn read_write(&self) -> (bool, bool) {
        if self.is_empty() {
            return (true, false);
        } else if self.contains(Self::WRONLY) {
            return (false, true);
        } else {
            return (true, true);
        }
    }
}
/// Open file with flags
pub fn open_file(name: &str, flags: OpenFlags) -> Option<Arc<OSInode>> {
    let (readable, writable) = flags.read_write();
    if flags.contains(OpenFlags::CREATE) {
        if let Some(inode) = ROOT_INODE.find(name) {
            //已经存在此文件名, 对应inode直接清空
            // clear size
            inode.clear();
            return Some(Arc::new(OSInode::new(readable, writable, inode)));
        } else {
            // Create file
            ROOT_INODE
                .create(name)
                .map(|inode| Arc::new(OSInode::new(readable, writable, inode)))
        }
    } else {
        ROOT_INODE.find(name).map(|inode| {
            if flags.contains(OpenFlags::TRUNC) {
                inode.clear();
            }
            Arc::new(OSInode::new(readable, writable, inode))
        })
    }
}

impl File for OSInode {
    fn readable(&self) -> bool {
        return self.readable;
    }

    fn writable(&self) -> bool {
        return self.writable;
    }
    fn read(&self, mut buf: UserBuffer) -> usize {
        let mut inner = self.inner.exclusive_access(); //加锁后拿
        let mut total_read_size = 0usize;
        for slice in buf.buffers.iter_mut() {
            let read_size = inner.inode.read_at(inner.offset, *slice);
            if read_size == 0 {
                break;
            }
            inner.offset += read_size;
            total_read_size += read_size;
        }
        return total_read_size;
    }
    fn write(&self, buf: UserBuffer) -> usize {
        let mut inner = self.inner.exclusive_access(); //加锁后拿
        let mut total_write_size = 0usize;
        for slice in buf.buffers.iter() {
            let write_size = inner.inode.write_at(inner.offset, *slice);
            assert_eq!(write_size, slice.len());
            inner.offset += write_size;
            total_write_size += write_size;
        }
        return total_write_size;
    }
}
