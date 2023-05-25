use core::fmt::Debug;

use alloc::{collections::VecDeque, sync::Arc};

use crate::task::{
    block_current_and_run_next, current_task, suspend_current_and_run_next, wakeup_task,
    TaskControlBlock,
};

use super::UPSafeCell;

pub trait Mutex: Sync + Send {
    fn lock(&self);
    fn unlock(&self);
}

impl Debug for dyn Mutex {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Mutex Lokcer",)
    }
}

pub struct MutexSpin {
    locked: UPSafeCell<bool>,
}

impl MutexSpin {
    pub fn new() -> Self {
        Self {
            locked: unsafe { UPSafeCell::new(false) },
        }
    }
}

impl Mutex for MutexSpin {
    fn lock(&self) {
        loop {
            let mut locked = self.locked.exclusive_access();
            if *locked {
                drop(locked);
                suspend_current_and_run_next();
                continue;
            } else {
                *locked = true;
                return;
            }
        }
    }

    fn unlock(&self) {
        let mut locked = self.locked.exclusive_access();
        *locked = false;
    }
}

pub struct MutexBlocking {
    inner: UPSafeCell<MutexBlockingInner>,
}

pub struct MutexBlockingInner {
    locked: bool,
    wait_queue: VecDeque<Arc<TaskControlBlock>>,
}

impl MutexBlocking {
    pub fn new() -> Self {
        Self {
            inner: unsafe {
                UPSafeCell::new(MutexBlockingInner {
                    locked: false,
                    wait_queue: VecDeque::new(),
                })
            },
        }
    }
}

impl Mutex for MutexBlocking {
    fn lock(&self) {
        let mut mutex_inner = self.inner.exclusive_access();
        if mutex_inner.locked {
            let tid = current_task()
                .unwrap()
                .inner_exclusive_access()
                .res
                .as_ref()
                .unwrap()
                .tid;
            println!("\x1b[34m[LOCK] thread locked tid [{}] \x1b[0m", tid);
            drop(tid);
            mutex_inner.wait_queue.push_back(current_task().unwrap()); //发现锁住了, 当前线程就进入blocked状态
            drop(mutex_inner);
            block_current_and_run_next();
        } else {
            println!("\x1b[34m[LOCK] 已经上锁 \x1b[0m" );
            mutex_inner.locked = true;
        }
    }

    fn unlock(&self) {
        let mut mutex_inner = self.inner.exclusive_access();
        assert!(mutex_inner.locked);
        if let Some(waking_task) = mutex_inner.wait_queue.pop_front() {
            //一个公平队列, 只唤醒最早进入blocked的线程
            wakeup_task(waking_task); //一个线程唤醒下一个线程. 所以每次只唤醒一个线程.
        } else {
            println!("\x1b[34m[UNLOCK] 锁现在已经打开 \x1b[0m");
            mutex_inner.locked = false; //没有等待这个锁的线程的话, 这个锁状态为打开
        }
    }
}
