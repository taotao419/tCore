
use super::pid::{KernelStack, PidHandle};
use super::TaskContext;
use crate::config::TRAP_CONTEXT;
use crate::mm::{MapPermission, MemorySet, PhysPageNum, VirtAddr, KERNEL_SPACE};
use crate::sync::UPSafeCell;
use crate::task::pid::pid_alloc;
use crate::trap::{trap_handler, TrapContext};
use alloc::sync::{Arc,Weak};
use alloc::vec::Vec;
use core::cell::RefMut;

/// task control block structure
#[derive(Debug)]
pub struct TaskControlBlock {
    //immutable
    pub pid: PidHandle,
    pub kernel_stack: KernelStack,
    //mutable
    inner: UPSafeCell<TaskControlBlockInner>,
}

#[derive(Debug)]
pub struct TaskControlBlockInner {
    pub trap_cx_ppn: PhysPageNum, //应用地址空间中的trap上下文 对应的物理页帧的页号
    pub base_size: usize, //应用数据仅有可能出现在应用地址低于base_size字节的区域中, 清楚知道多少数据驻留在内存中.
    pub task_cx: TaskContext, //暂停任务上下文保持在此
    pub task_status: TaskStatus, //执行状态
    pub memory_set: MemorySet, //应用地址空间
    pub parent: Option<Weak<TaskControlBlock>>, //父进程
    pub children: Vec<Arc<TaskControlBlock>>, //多个子进程
    pub exit_code: i32,   //当进程主动调用exit 或者执行出错被内核杀死, 它的退出码会不同
}

impl TaskControlBlockInner {
    pub fn get_trap_cx(&self) -> &'static mut TrapContext {
        return self.trap_cx_ppn.get_mut();
    }

    pub fn get_user_token(&self) -> usize {
        return self.memory_set.token();
    }

    fn get_status(&self) -> TaskStatus {
        return self.task_status;
    }

    pub fn is_zombie(&self) -> bool {
        return self.get_status() == TaskStatus::Zombie;
    }
}

impl TaskControlBlock {
    pub fn inner_exclusive_access(&self) -> RefMut<'_, TaskControlBlockInner> {
        self.inner.exclusive_access()
    }
    pub fn new(elf_data: &[u8]) -> Self {
        // memory_set with elf program headers/trampoline/trap context/user stack
        let (memory_set, user_sp, entry_point) = MemorySet::from_elf(elf_data);
        let trap_cx_ppn = memory_set
            .translate(VirtAddr::from(TRAP_CONTEXT).into())
            .unwrap()
            .ppn();
        //alloc a pid and a kernel stack in kernel space
        let pid_handle = pid_alloc();
        let kernel_stack = KernelStack::new(&pid_handle);
        let kernel_stack_top = kernel_stack.get_top();
        // push a task context which goes to trap_return to the top of kernel stack
        let task_control_block = Self {
            pid: pid_handle,
            kernel_stack,
            inner: unsafe {
                UPSafeCell::new(TaskControlBlockInner {
                    trap_cx_ppn,
                    base_size: user_sp,
                    task_cx: TaskContext::goto_trap_return(kernel_stack_top),
                    task_status: TaskStatus::Ready,
                    memory_set,
                    parent: None,
                    children: Vec::new(),
                    exit_code: 0,
                })
            },
        };

        // prepare TrapContext in user space
        let trap_cx = task_control_block.inner_exclusive_access().get_trap_cx();
        *trap_cx = TrapContext::app_init_context(
            entry_point,
            user_sp,
            KERNEL_SPACE.exclusive_access().token(),
            kernel_stack_top,
            trap_handler as usize,
        );
        println!(
            "Process id : [{}]  new task control block : [{:#?}]",
            task_control_block.pid.0, task_control_block
        );
        task_control_block
    }

    pub fn fork(self: &Arc<Self>) -> Arc<Self> {
        let mut parent_inner = self.inner_exclusive_access();
        //copy user space(include trap context)
        let memory_set = MemorySet::from_existed_user(&parent_inner.memory_set);
        let trap_cx_ppn = memory_set
            .translate(VirtAddr::from(TRAP_CONTEXT).into())
            .unwrap()
            .ppn();
        //alloc a pid and a kernel stack in kernel space
        let pid_handle = pid_alloc();
        let kernel_stack = KernelStack::new(&pid_handle);
        let kernel_stack_top = kernel_stack.get_top();
        let task_control_block = Arc::new(TaskControlBlock {
            pid: pid_handle,
            kernel_stack,
            inner: unsafe {
                UPSafeCell::new(TaskControlBlockInner {
                    trap_cx_ppn,
                    base_size: parent_inner.base_size,
                    task_cx: TaskContext::goto_trap_return(kernel_stack_top),
                    task_status: TaskStatus::Ready,
                    memory_set,
                    parent: Some(Arc::downgrade(self)),
                    children: Vec::new(),
                    exit_code: 0,
                })
            },
        });
        //add child
        parent_inner.children.push(task_control_block.clone());
        // modify kernel_sp in trap_cx
        // **** access children PCB exclusively
        let trap_cx = task_control_block.inner_exclusive_access().get_trap_cx();
        trap_cx.kernel_sp = kernel_stack_top;
        return task_control_block;
        // ---- release parent PCB automatically
        // **** release children PCB automatically
    }

    pub fn exec(&self, elf_data: &[u8]) {
        // memory_set with elf program headers/trampoline/trap context/user stack
        let (memory_set, user_sp, entry_point) = MemorySet::from_elf(elf_data);
        let trap_cx_ppn = memory_set
            .translate(VirtAddr::from(TRAP_CONTEXT).into())
            .unwrap()
            .ppn();

        let mut inner = self.inner_exclusive_access();
        inner.memory_set = memory_set;
        inner.trap_cx_ppn = trap_cx_ppn;
        inner.base_size = user_sp;
        let trap_cx = inner.get_trap_cx();
        *trap_cx = TrapContext::app_init_context(
            entry_point,
            user_sp,
            KERNEL_SPACE.exclusive_access().token(),
            self.kernel_stack.get_top(),
            trap_handler as usize,
        );
    }

    pub fn getpid(&self) -> usize {
        return self.pid.0;
    }
}
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum TaskStatus {
    Ready,
    Running,
    Zombie,
}
