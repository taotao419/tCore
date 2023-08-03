use crate::sync::{Mutex, UPSafeCell,UPIntrFreeCell};
use crate::task::{block_current_and_run_next, current_task, wakeup_task, TaskControlBlock, TaskContext, block_current_task};
use alloc::{collections::VecDeque, sync::Arc};

#[derive(Debug)]
pub struct Condvar {
    pub inner: UPIntrFreeCell<CondvarInner>,
}

#[derive(Debug)]
pub struct CondvarInner {
    pub wait_queue: VecDeque<Arc<TaskControlBlock>>,
}

impl Condvar {
    pub fn new() -> Self {
        Self {
            inner: unsafe {
                UPIntrFreeCell::new(CondvarInner {
                    wait_queue: VecDeque::new(),
                })
            },
        }
    }

    pub fn signal(&self) {
        let mut inner = self.inner.exclusive_access();
        if let Some(task) = inner.wait_queue.pop_front() {
            wakeup_task(task);
        }
    }

    pub fn signal_all(&self) {
        let mut inner = self.inner.exclusive_access();
        while let Some(task) = inner.wait_queue.pop_front() {
            wakeup_task(task);
        }
    }

    pub fn wait(&self, mutex: Arc<dyn Mutex>) {
        mutex.unlock();
        let mut inner = self.inner.exclusive_access(); // lock and get
        inner.wait_queue.push_back(current_task().unwrap());
        drop(inner); // unlock inner's lock
        block_current_and_run_next();
        mutex.lock();
    }

    // 只是单纯把当前 线程/进程 执行状态字段改为 block. 并没有真正把当前 线程/进程 休眠
    pub fn wait_no_sched(&self) -> *mut TaskContext {
        self.inner.exclusive_session(|inner|{
            inner.wait_queue.push_back(current_task().unwrap());
        });
        return block_current_task();
    }
}
