use super::pid::{KernelStack, PidHandle};
use super::TaskContext;
use crate::config::TRAP_CONTEXT;
use crate::fs::{File, Stdin, Stdout};
use crate::logger::{info, info2};
use crate::mm::{translated_str,translated_refmut, MapPermission, MemorySet, PhysPageNum, VirtAddr, KERNEL_SPACE};
use crate::sync::UPSafeCell;
use crate::task::pid::pid_alloc;
use crate::trap::{trap_handler, TrapContext};
use alloc::string::{String, ToString};
use alloc::sync::{Arc, Weak};
use alloc::vec;
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
    pub app_name: String,
    pub parent: Option<Weak<TaskControlBlock>>, //父进程
    pub children: Vec<Arc<TaskControlBlock>>,   //多个子进程
    pub exit_code: i32, //当进程主动调用exit 或者执行出错被内核杀死, 它的退出码会不同
    pub fd_table: Vec<Option<Arc<dyn File + Send + Sync>>>, //文件描述符表
    pub working_dir: String, //当前工作目录
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

    pub fn alloc_fd(&mut self) -> usize {
        if let Some(fd) = (0..self.fd_table.len()).find(|fd| self.fd_table[*fd].is_none()) {
            return fd;
        } else {
            self.fd_table.push(None);
            return self.fd_table.len() - 1;
        }
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
                    app_name: String::new(),
                    children: Vec::new(),
                    exit_code: 0,
                    fd_table: vec![
                        // 0 -> stdin
                        Some(Arc::new(Stdin)),
                        // 1 -> stdout
                        Some(Arc::new(Stdout)),
                        // 2 -> stderr
                        Some(Arc::new(Stdout)),
                    ],
                    working_dir: String::from("/"),
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
        info(
            "App name : init_proc , Process id : ",
            &task_control_block.pid.0,
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
        // copy fd table
        let mut new_fd_table: Vec<Option<Arc<dyn File + Send + Sync>>> = Vec::new();
        for fd in parent_inner.fd_table.iter() {
            if let Some(file) = fd {
                new_fd_table.push(Some(file.clone()));
            } else {
                new_fd_table.push(None);
            }
        }
        let working_dir = parent_inner.working_dir.clone();
        // println!("[KERNEL] Fork parent working dir : {}", &working_dir);
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
                    app_name: String::new(),
                    parent: Some(Arc::downgrade(self)),
                    children: Vec::new(),
                    exit_code: 0,
                    fd_table: new_fd_table,
                    working_dir,
                })
            },
        });
        //add child
        parent_inner.children.push(task_control_block.clone());
        // modify kernel_sp in trap_cx
        // **** access children PCB exclusively
        let trap_cx = task_control_block.inner_exclusive_access().get_trap_cx();
        trap_cx.kernel_sp = kernel_stack_top;
        info("[KERNEL] Fork parent Process id : ", &self.getpid());
        info(
            "[KERNEL] Fork new process id : ",
            &task_control_block.getpid(),
        );
        return task_control_block;
        // ---- release parent PCB automatically
        // **** release children PCB automatically
    }

    pub fn exec(&self, app_name: &str, elf_data: &[u8], args: Vec<String>) {
        // memory_set with elf program headers/trampoline/trap context/user stack
        let (memory_set, mut user_sp, entry_point) = MemorySet::from_elf(elf_data);
        let trap_cx_ppn = memory_set
            .translate(VirtAddr::from(TRAP_CONTEXT).into())
            .unwrap()
            .ppn();
        // push arguments on user stack
        // ex: 传入两个参数 aa和bb
        //                                          argv_base-----|
        //                                |<--8 bytes->|          v   |1B |
        // HighAddr |         0           |  argv[1]   |  argv[0] |\0| a | a |\0| b | b |Alignment|  LowAddr
        //          ^--user_sp (original)      |            |--------------^                      ^---user_sp (now)
        //                                     |--------------------------------------^
        // 1 usize == 8 bytes
        println!("\x1b[32m[SYSCALL : exec] original user_sp : [{}] \x1b[0m", user_sp);
        user_sp -= (args.len() + 1) * core::mem::size_of::<usize>(); // 先把user_sp栈顶指针下压到 arg[0] 处 , 这里压3个usize 0+arg[1]+arg[0]
        let argv_base = user_sp;
        let mut argv: Vec<_> = (0..=args.len())
            .map(|arg| {
                //ex: argv[0] 的值是入参字符串头字符指针.
                translated_refmut(
                    memory_set.token(),
                    (argv_base + arg * core::mem::size_of::<usize>()) as *mut usize,
                )
            })
            .collect();
        *argv[args.len()] = 0; //示例中argv[2]=0
                               //这里argv 还只是place holder, 还没有真正赋值

        for i in 0..args.len() {
            user_sp -= args[i].len() + 1;
            *argv[i] = user_sp; // ex: argv[0] 指向 "aa" 这个首字母的位置 , 现在赋值了
            let mut p = user_sp;
            //这里把aa这两个字符,压到栈里. 也就是写入p指向位置的格子(byte)
            for c in args[i].as_bytes() {
                *translated_refmut(memory_set.token(), p as *mut u8) = *c;
                p += 1; //下一个字节位置
            }
              
            *translated_refmut(memory_set.token(), p as *mut u8) = 0; //把\0压入栈里,方便应用知道哪里是字符串结尾
            println!(
                "\x1b[32m[SYSCALL : exec] argv[{}] point [{}] ==> string [{}]    \x1b[0m",
                i,
                argv[i],
                translated_str(memory_set.token(), *argv[i] as *const u8)
            );
        }
        println!(
            "\x1b[32m[SYSCALL : exec] user stack argv array -- [{:?}] \x1b[0m",
            argv
        );
        // make the user_sp aligned to 8B for k210 platform
        user_sp -= user_sp % core::mem::size_of::<usize>(); //K210平台上访问用户栈会触发访存不对齐的异常 也就是Alignment这块地址

        let mut inner = self.inner_exclusive_access();
        inner.memory_set = memory_set;
        inner.trap_cx_ppn = trap_cx_ppn;
        inner.base_size = user_sp; // 是不是要这一行?
        inner.app_name = app_name.to_string();
        // initialize trap_cx
        let mut trap_cx = TrapContext::app_init_context(
            entry_point,
            user_sp,
            KERNEL_SPACE.exclusive_access().token(),
            self.kernel_stack.get_top(),
            trap_handler as usize,
        );
        trap_cx.x[10] = args.len(); // x10(a0)寄存器, 函数入参1寄存器 传入命令行参数个数
        trap_cx.x[11] = argv_base; // x11(a1)寄存器, 函数入参2寄存器 传如argv_base 具体位置看上面注释图 读取argv[0]/argv[1]/argv[2]
        *inner.get_trap_cx() = trap_cx;

        println!("\x1b[32m[SYSCALL : exec] now user_sp : [{}] \x1b[0m", user_sp);
        info("[KERNEL] EXEC Process id : ", &self.getpid());
        info("[KERNEL] EXEC App name : ", &inner.app_name);
    }

    pub fn getpid(&self) -> usize {
        return self.pid.0;
    }

    pub fn get_working_dir(&self) -> String {
        return self.inner_exclusive_access().working_dir.clone();
    }
}
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum TaskStatus {
    Ready,
    Running,
    Zombie,
}
