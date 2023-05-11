use super::id::TaskUserRes;
use super::{kstack_alloc, KernelStack, ProcessControlBlock, TaskContext};
use crate::trap::TrapContext;
use crate::{mm::PhysPageNum, sync::UPSafeCell};
use alloc::sync::{Arc, Weak};
use core::cell::RefMut;

/// task control block structure
#[derive(Debug)]
pub struct TaskControlBlock {
    //immutable
    pub process: Weak<ProcessControlBlock>,
    pub kstack: KernelStack,
    //mutable
    inner: UPSafeCell<TaskControlBlockInner>,
}

#[derive(Debug)]
pub struct TaskControlBlockInner {
    pub res: Option<TaskUserRes>,
    pub trap_cx_ppn: PhysPageNum, //应用地址空间中的trap上下文 对应的物理页帧的页号
    pub task_cx: TaskContext,     //暂停任务上下文保持在此
    pub task_status: TaskStatus,  //执行状态
    pub exit_code: Option<i32>,   //当进程主动调用exit 或者执行出错被内核杀死, 它的退出码会不同
    pub trap_ctx_backup: Option<TrapContext>,
}

impl TaskControlBlockInner {
    pub fn get_trap_cx(&self) -> &'static mut TrapContext {
        return self.trap_cx_ppn.get_mut();
    }

    fn get_status(&self) -> TaskStatus {
        return self.task_status;
    }
}

impl TaskControlBlock {
    pub fn inner_exclusive_access(&self) -> RefMut<'_, TaskControlBlockInner> {
        self.inner.exclusive_access()
    }

    pub fn get_user_token(&self)-> usize {
        let process=self.process.upgrade().unwrap();
        let inner=process.inner_exclusive_access();
        return inner.memory_set.token();
    }

    pub fn new(
        process: Arc<ProcessControlBlock>,
        ustack_base: usize,
        alloc_user_res: bool,
    ) -> Self {
        let res = TaskUserRes::new(Arc::clone(&process), ustack_base, alloc_user_res);
        let trap_cx_ppn = res.trap_cx_ppn();
        let kstack = kstack_alloc(); //创建新线程 除了用户栈之外 还必须创建内核栈
        let kstack_top = kstack.get_top();
        Self {
            process: Arc::downgrade(&process),
            kstack,
            inner: unsafe {
                UPSafeCell::new(TaskControlBlockInner {
                    res: Some(res),
                    trap_cx_ppn,
                    task_cx: TaskContext::goto_trap_return(kstack_top),
                    task_status: TaskStatus::Ready,
                    exit_code: None,
                    trap_ctx_backup: None,
                })
            },
        }
    }

}
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum TaskStatus {
    Ready,
    Running,
    Blocked,
}
