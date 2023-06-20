use crate::mm::UserBuffer;
use crate::sync::{Mutex, Semaphore, UPSafeCell};
use crate::task::{block_current_and_run_next, current_task, wakeup_task, TaskControlBlock};
use alloc::{collections::VecDeque, sync::Arc};
use bitflags::*;

use super::File;

#[derive(Debug)]
pub struct Eventfd {
    non_block: bool, // 非阻塞模式
    semaphore: bool, // 信号量模式
    pub inner: UPSafeCell<EventfdInner>,
}

#[derive(Debug)]
pub struct EventfdInner {
    pub count: usize,
    pub wait_queue: VecDeque<Arc<TaskControlBlock>>,
}

const U32_BYTE_SIZE: usize = 4;

bitflags! {
    /// Eventfd flags
    pub struct EventfdFlags:u32 {
        const DEFAULT = 0;
        ///SEMAPHORE mode
        const SEMAPHORE = 1<<0;
         ///Non Block
        const NONBLOCK = 1<<11;
    }
}

impl Eventfd {
    pub fn new(non_block: bool, semaphore: bool, count: usize) -> Self {
        Self {
            non_block,
            semaphore,
            inner: unsafe {
                UPSafeCell::new(EventfdInner {
                    count,
                    wait_queue: VecDeque::new(),
                })
            },
        }
    }
}

pub fn eventfd_create(initval: usize, flags: EventfdFlags) -> Option<Arc<Eventfd>> {
    let (non_block, semaphore) = flags.nonblock_semaphore();
    return Some(Arc::new(Eventfd::new(non_block, semaphore, initval)));
}

impl EventfdFlags {
    pub fn nonblock_semaphore(&self) -> (bool, bool) {
        if self.contains(Self::NONBLOCK) {
            return (true, false);
        } else if self.contains(Self::SEMAPHORE) {
            return (false, true);
        } else if self.contains(Self::DEFAULT) {
            return (false, false);
        } else {
            panic!("wrong eventfd flags")
        }
    }
}

impl File for Eventfd {
    fn readable(&self) -> bool {
        return true;
    }

    fn writable(&self) -> bool {
        // 按照Linux Programmer Manual规定 : count 大于 0xffffffffffffffff时不可写
        // 我们这里就简化为 可写
        return true;
    }

    fn read(&self, buf: UserBuffer) -> usize {
        let mut count = self.inner.exclusive_access().count as u32;
        while count == 0 {
            log!("\x1b[38;5;208m[KERNEL EVENTFD] READ count=[0]\x1b[0m");
            if self.non_block == true {
                println!("Non block mode , no resource");
                return 0;
            } else {
                let mut inner = self.inner.exclusive_access(); //lock II
                inner.wait_queue.push_back(current_task().unwrap());
                drop(inner); // unlock II
                log!("\x1b[38;5;208m[KERNEL EVENTFD] count == 0 ,block current thread\x1b[0m");
                block_current_and_run_next();
                count = self.inner.exclusive_access().count as u32;
            }
        }

        // 线程堵塞后又开始继续执行的情况 or count > 0
        log!( "\x1b[38;5;208m[KERNEL EVENTFD] READ count=[{}]\x1b[0m", count);
        let mut bytes: [u8; U32_BYTE_SIZE] = [0; U32_BYTE_SIZE];
        let mut inner = self.inner.exclusive_access(); //lock III
        if self.semaphore == true {
            //if semephoe == true return &buf = 1 and counter--
            bytes = (1 as u32).to_be_bytes();
            inner.count -= 1;
        } else {
            //if semephoe == false return &buf = counter and counter = 0
            bytes = count.to_be_bytes(); //默认大端字节序列
            inner.count = 0;
        }

        let mut buf_iter = buf.into_iter();
        for i in 0..U32_BYTE_SIZE {
            if let Some(byte_ref) = buf_iter.next() {
                unsafe {
                    *byte_ref = bytes[i];
                }
            }
        }
        return U32_BYTE_SIZE; // 4 bytes & unlock III
    }

    fn write(&self, buf: UserBuffer) -> usize {
        assert_eq!(buf.len(), U32_BYTE_SIZE); // 确保buf 长度是 4.
        let mut inner = self.inner.exclusive_access(); //lock I
        if self.semaphore == true {
            inner.count += 1;
        } else {
            let mut bytes: [u8; U32_BYTE_SIZE] = [0; U32_BYTE_SIZE];
            let mut buf_iter = buf.into_iter();
            for i in 0..U32_BYTE_SIZE {
                if let Some(byte_ref) = buf_iter.next() {
                    unsafe {
                        bytes[i] = *byte_ref;
                    }
                }
            }
            inner.count = inner.count + u32::from_be_bytes(bytes) as usize;
            log!(
                "\x1b[38;5;208m[KERNEL EVENTFD] WRITE count=[{}]\x1b[0m",
                inner.count
            );
        }

        if let Some(task) = inner.wait_queue.pop_front() {
            wakeup_task(task);
        }

        return U32_BYTE_SIZE; // 4 bytes & unlock I
    }
}
