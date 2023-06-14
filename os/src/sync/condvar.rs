use crate::sync::{Mutex, UPSafeCell};
use crate::task::{block_current_and_run_next, current_task, wakeup_task, TaskControlBlock};
use alloc::{collections::VecDeque, sync::Arc};

#[derive(Debug)]
pub struct Condvar {
    pub inner: UPSafeCell<CondvarInner>,
}

#[derive(Debug)]
pub struct CondvarInner {
    pub wait_queue: VecDeque<Arc<TaskControlBlock>>,
}

impl Condvar {
    pub fn new() -> Self {
        Self {
            inner: unsafe {
                UPSafeCell::new(CondvarInner {
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
}
