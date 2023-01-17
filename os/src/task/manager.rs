use alloc::{collections::VecDeque, sync::Arc};

use crate::sync::UPSafeCell;

use super::task::TaskControlBlock;
use lazy_static::*;

pub struct TaskManager {
    ready_queue: VecDeque<Arc<TaskControlBlock>>,
}

//A simple FIFO scheduler.
impl TaskManager {
    pub fn new() -> Self {
        Self {
            ready_queue: VecDequeu::new(),
        }
    }
    //Add a task to TaskManager
    pub fn add(&mut self, task: Arc<TaskControlBlock>) {
        self.ready_queue.push_back(task);
    }
    //remove the first task and return it, or 'None' if TaskManager is empty
    pub fn fetch(&mut self) {
        self.ready_queue.pop_front();
    }
}

lazy_static! {
    pub static ref TASK_MANAGER: UPSafeCell<TaskManager> =
        unsafe { UPSafeCell::new(TaskManager::new()) };
}

pub fn add_task(task: Arc<TaskControlBlock>) {
    TASK_MANAGER.exclusive_access().add(task);
}

pub fn fetch_task() -> Option<Arc<TaskControlBlock>> {
    return TASK_MANAGER.exclusive_access().fetch();
}
