//! Task management implementation
//!
//! Everything about task management, like starting and switching tasks is
//! implemented here.
//!
//! A single global instance of [`TaskManager`] called `TASK_MANAGER` controls
//! all the tasks in the operating system.
//!
//! Be careful when you see `__switch` ASM function in `switch.S`. Control flow around this function
//! might not be what you expect.

mod context;
mod manager;
mod pid;
mod processor;
mod switch;
#[allow(clippy::module_inception)]
mod task;

use crate::loader::{get_app_data, get_app_data_by_name, get_num_app};
use crate::logger::{self, info, warn};
use crate::sync::UPSafeCell;
use crate::timer::get_time_ms;
use crate::trap::TrapContext;
use alloc::sync::Arc;
use alloc::vec::Vec;
use lazy_static::*;
use switch::__switch;
use task::{TaskControlBlock, TaskStatus};

pub use context::TaskContext;
pub use manager::add_task;
pub use processor::{
    current_task, current_trap_cx, current_user_token, run_tasks, schedule, take_current_task,
    Processor,
};


/// Suspend the current 'Running' task and run the next task in task list.
pub fn suspend_current_and_run_next() {
    let task = take_current_task().unwrap();

    //-- access current TCB exclusively
    let mut task_inner = task.inner_exclusive_access();
    let task_cx_ptr = &mut task_inner.task_cx as *mut TaskContext;
    //Change status to Ready
    task_inner.task_status = TaskStatus::Ready;
    drop(task_inner);
    // ---- stop exclusively accessing current PCB

    //push back to ready queue
    add_task(task);
    //jump to scheduling cycle
    schedule(task_cx_ptr); //它会调用__switch 汇编函数(核心函数) 真正切换任务
}

/// pid of usertests app in make run TEST=1
pub const IDLE_PID: usize = 0;
/// Exit the current 'Running' task and run the next task in task list.
pub fn exit_current_and_run_next(exit_code: i32) {
    // take from Processor
    let task = take_current_task().unwrap();
    let pid = task.getpid();
    if pid == IDLE_PID {
        println!(
            "[kernel] Idle process exit with exit_code {} ...",
            exit_code
        );
        if exit_code != 0 {
            println!("[kernel] user app exit with exit_code {} ...", exit_code);
            crate::sbi::shutdown()
        } else {
            println!("[kernel] user app exit success ...");
            crate::sbi::shutdown()
        }
    }

    let mut inner = task.inner_exclusive_access();
    inner.task_status = TaskStatus::Zombie;
    inner.exit_code = exit_code;
    // do not move to its parent but under initproc

    // ++++++ access initproc TCB exclusively
    {
        let mut initproc_inner = INITPROC.inner_exclusive_access();
        for child in inner.children.iter() {
            child.inner_exclusive_access().parent = Some(Arc::downgrade(&INITPROC));
            initproc_inner.children.push(child.clone());
        }
    }
    // ++++++ release parent PCB

    inner.children.clear();
    inner.memory_set.recycle_data_pages();
    drop(inner);
    drop(task);
    // we do not have to save task context
    let mut _unused = TaskContext::zero_init();
    schedule(&mut _unused as *mut _);
}

lazy_static! {
    //Global process that init user shell
    pub static ref INITPROC:Arc<TaskControlBlock> = Arc::new(TaskControlBlock::new(get_app_data_by_name("initproc").unwrap()));
}
///Add init process to the manager
pub fn add_initproc() {
    add_task(INITPROC.clone());
}
