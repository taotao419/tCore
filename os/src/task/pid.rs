use crate::{
    config::{KERNEL_STACK_SIZE, PAGE_SIZE,TRAMPOLINE},
    mm::{VirtAddr, KERNEL_SPACE, MapPermission},
    task::UPSafeCell,
};
use alloc::vec::Vec;
use lazy_static::*;
use lazy_static::*;

pub struct PidAllocator {
    current: usize,
    recycled: Vec<usize>,
}

impl PidAllocator {
    //Create an empty 'PidAllocator'
    pub fn new() -> Self {
        return PidAllocator {
            current: 0,
            recycled: Vec::new(),
        };
    }
    //Allocate a pid
    pub fn alloc(&mut self) -> PidHandle {
        if let Some(pid) = self.recycled.pop() {
            PidHandle(pid)
        } else {
            self.current += 1;
            PidHandle(self.current - 1)
        }
    }
    //Recycle a pid
    pub fn dealloc(&mut self, pid: usize) {
        assert!(pid < self.current);
        assert!(
            !self.recycled.iter().any(|ppid| *ppid == pid),
            "pid {} has been deallocated!",
            pid
        );
        self.recycled.push(pid);
    }
}

lazy_static! {
    pub static ref PID_ALLOCATOR: UPSafeCell<PidAllocator> =
        unsafe { UPSafeCell::new(PidAllocator::new()) };
}

///Bind pid lifetime to `PidHandle`
#[derive(Debug)]
pub struct PidHandle(pub usize);

impl Drop for PidHandle {
    fn drop(&mut self) {
        println!("drop pid {}", self.0);
        PID_ALLOCATOR.exclusive_access().dealloc(self.0);
    }
}

///Allocate a pid from PID_ALLOCATOR
pub fn pid_alloc() -> PidHandle {
    PID_ALLOCATOR.exclusive_access().alloc()
}

//Return (bottom, top) of a kernel stack in kernel space.
pub fn kernel_stack_position(app_id: usize) -> (usize, usize) {
    let top = TRAMPOLINE - app_id * (KERNEL_STACK_SIZE + PAGE_SIZE);
    let bottom = top - KERNEL_STACK_SIZE;
    return (bottom, top);
}

//Kernelstack for app
#[derive(Debug)]
pub struct KernelStack {
    pid: usize,
}

impl KernelStack {
    // **关键点** 在加载每个用户程序时,还会在核心kernel 创建核心栈
    pub fn new(pid_handle: &PidHandle) -> Self {
        let pid = pid_handle.0;
        let (kernel_stack_bottom, kernel_stack_top) = kernel_stack_position(pid);
        KERNEL_SPACE.exclusive_access().insert_framed_area(
            kernel_stack_bottom.into(),
            kernel_stack_top.into(),
            MapPermission::R | MapPermission::W,
        );
        // println!(
        //     "[KERNEL] mapping stack section for APP:{}  [{:#x}, {:#x})",
        //     pid, kernel_stack_bottom, kernel_stack_top
        // );
        return KernelStack { pid: pid_handle.0 };
    }

    #[allow(unused)]
    //将一个类型为T的变量压入内核栈并返回裸指针
    pub fn push_on_top<T>(&self, value: T) -> *mut T
    where
        T: Sized,
    {
        let kernel_stack_top = self.get_top();
        let ptr_mut = (kernel_stack_top - core::mem::size_of::<T>()) as *mut T;
        unsafe {
            *ptr_mut = value;
        }
        return ptr_mut;
    }

    //获得指定进程id对应的内核栈顶
    pub fn get_top(&self) -> usize {
        let (_, kernel_stack_top) = kernel_stack_position(self.pid);
        return kernel_stack_top;
    }
}

impl Drop for KernelStack {
    fn drop(&mut self) {
        let (kernel_stack_bottom, _) = kernel_stack_position(self.pid);
        let kernel_stack_bottom_va: VirtAddr = kernel_stack_bottom.into();
        KERNEL_SPACE
            .exclusive_access()
            .remove_area_with_start_vpn(kernel_stack_bottom_va.into());
    }
}
