use super::__switch;
use super::{fetch_task, TaskStatus};
use super::{ProcessControlBlock, TaskContext, TaskControlBlock};
use crate::sync::UPSafeCell;
use crate::trap::TrapContext;
use alloc::sync::Arc;
use lazy_static::*;

pub struct Processor {
    //表示当前处理器上正在执行的任务
    current: Option<Arc<TaskControlBlock>>,
    //表示当前处理器上的idle 控制流的任务上下文
    idle_task_cx: TaskContext,
}

impl Processor {
    pub fn new() -> Self {
        Self {
            current: None,
            idle_task_cx: TaskContext::zero_init(),
        }
    }

    fn get_idle_task_cx_ptr(&mut self) -> *mut TaskContext {
        &mut self.idle_task_cx as *mut _
    }

    //从Processor结构里拿走了, 拿走了Processor里面就是空值了
    pub fn take_current(&mut self) -> Option<Arc<TaskControlBlock>> {
        self.current.take()
    }

    //从Processor结构取值但不拿走了, 只是复制了一份返回. Processor保持原样
    pub fn current(&self) -> Option<Arc<TaskControlBlock>> {
        return self.current.as_ref().map(Arc::clone);
    }
}

lazy_static! {
    pub static ref PROCESSOR: UPSafeCell<Processor> = unsafe { UPSafeCell::new(Processor::new()) };
}
//The main part of process execution and scheduling
//循环 fetch_task 把一个任务从TaskManager这个链表拿出来 放到PROCESSOR里面去. 然后调用__switch切换到这个任务
pub fn run_tasks() {
    loop {
        let mut processor = PROCESSOR.exclusive_access(); //对于单核CPU来说有点脱裤子放屁
        if let Some(task) = fetch_task() {
            let idle_task_cx_ptr = processor.get_idle_task_cx_ptr(); //这个本质就是current_task_cx
            let mut task_inner = task.inner_exclusive_access();
            let next_task_cx_ptr = &task_inner.task_cx as *const TaskContext; //为了配合__switch这个函数 所以叫next_task_cx
            task_inner.task_status = TaskStatus::Running;
            drop(task_inner);
            // release coming task TCB manually
            processor.current = Some(task);
            // release processor manually
            drop(processor);
            unsafe {
                __switch(idle_task_cx_ptr, next_task_cx_ptr);
            }
        } else {
            println!("no tasks available in run_tasks");
        }
    }
}

///Take the current thread,leaving a None in its place
pub fn take_current_task() -> Option<Arc<TaskControlBlock>> {
    return PROCESSOR.exclusive_access().take_current();
}
///Get running thread
pub fn current_task() -> Option<Arc<TaskControlBlock>> {
    return PROCESSOR.exclusive_access().current();
}

pub fn current_process() -> Arc<ProcessControlBlock> {
    return current_task().unwrap().process.upgrade().unwrap();
}

pub fn current_user_token() -> usize {
    let task = current_task().unwrap();
    return task.get_user_token();
}

pub fn current_trap_cx() -> &'static mut TrapContext {
    return current_task()
        .unwrap()
        .inner_exclusive_access()
        .get_trap_cx();
}

pub fn current_trap_cx_user_va() -> usize {
    return current_task()
        .unwrap()
        .inner_exclusive_access()
        .res
        .as_ref()
        .unwrap()
        .trap_cx_user_va();
}

pub fn current_kstack_top() -> usize {
    return current_task().unwrap().kstack.get_top();
}

//Return to idle control flow for new scheduling
pub fn schedule(switched_task_cx_ptr: *mut TaskContext) {
    let mut processor = PROCESSOR.exclusive_access();
    let idle_task_cx_ptr = processor.get_idle_task_cx_ptr();
    drop(processor);
    unsafe {
        __switch(switched_task_cx_ptr, idle_task_cx_ptr);
    }
}
