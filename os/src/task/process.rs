use crate::fs::{File, Stdin, Stdout};
use crate::mm::{translated_refmut, translated_str, MemorySet, KERNEL_SPACE};
use crate::sync::{UPSafeCell, Mutex, Semaphore, Condvar};
use crate::trap::{trap_handler, TrapContext};

use super::id::{PidHandle, RecycleAllocator};
use super::manager::insert_into_pid2process;
use super::task::TaskControlBlock;
use super::{add_task, pid_alloc, SignalActions, SignalFlags};
use alloc::string::{String, ToString};
use alloc::sync::{Arc, Weak};
use alloc::vec;
use alloc::vec::Vec;
use core::cell::RefMut;

#[derive(Debug)]
pub struct ProcessControlBlock {
    pub pid: PidHandle,
    inner: UPSafeCell<ProcessControlBlockInner>,
}

#[derive(Debug)]
pub struct ProcessControlBlockInner {
    pub app_name: String,
    pub working_dir: String,
    pub is_zombie: bool,
    pub memory_set: MemorySet,                              //应用地址空间
    pub parent: Option<Weak<ProcessControlBlock>>,          //父进程
    pub children: Vec<Arc<ProcessControlBlock>>,            //多个子进程
    pub exit_code: i32, //当进程主动调用exit 或者执行出错被内核杀死, 它的退出码会不同
    pub fd_table: Vec<Option<Arc<dyn File + Send + Sync>>>, //文件描述符表
    pub signals: SignalFlags, //记录目前已经收到了哪些尚未处理的信号
    pub signal_mask: SignalFlags,
    pub handling_sig: isize,           // the signal which is being handling
    pub signal_actions: SignalActions, // Signal actions /
    pub killed: bool,                  // if the task is killed
    pub frozen: bool,                  // if the task is frozen by a signal
    pub tasks: Vec<Option<Arc<TaskControlBlock>>>,
    pub task_res_allocator: RecycleAllocator,
    pub mutex_list: Vec<Option<Arc<dyn Mutex>>>,
    pub semaphore_list: Vec<Option<Arc<Semaphore>>>,
    pub condvar_list: Vec<Option<Arc<Condvar>>>,
}

impl ProcessControlBlockInner {
    pub fn alloc_fd(&mut self) -> usize {
        if let Some(fd) = (0..self.fd_table.len()).find(|fd| self.fd_table[*fd].is_none()) {
            return fd;
        } else {
            self.fd_table.push(None);
            return self.fd_table.len() - 1;
        }
    }

    pub fn alloc_tid(&mut self) -> usize {
        return self.task_res_allocator.alloc();
    }

    pub fn dealloc_tid(&mut self, tid: usize) {
        self.task_res_allocator.dealloc(tid);
    }

    pub fn thread_count(&self) -> usize {
        return self.tasks.len();
    }

    pub fn get_task(&self, tid: usize) -> Arc<TaskControlBlock> {
        return self.tasks[tid].as_ref().unwrap().clone();
    }
}

impl ProcessControlBlock {
    pub fn inner_exclusive_access(&self) -> RefMut<'_, ProcessControlBlockInner> {
        self.inner.exclusive_access()
    }

    pub fn new(elf_data: &[u8]) -> Arc<Self> {
        // memory_set with elf program headers/trampoline/trap context/user stack
        let (memory_set, ustack_base, entry_point) = MemorySet::from_elf(elf_data);
        //alloc a pid and a kernel stack in kernel space
        let pid_handle = pid_alloc();
        // push a task context which goes to trap_return to the top of kernel stack
        let process = Arc::new(Self {
            pid: pid_handle,
            inner: unsafe {
                UPSafeCell::new(ProcessControlBlockInner {
                    app_name: String::new(),
                    working_dir: String::new(),
                    is_zombie: false,
                    memory_set,
                    parent: None,
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
                    signals: SignalFlags::empty(),
                    signal_mask: SignalFlags::empty(),
                    handling_sig: -1,
                    signal_actions: SignalActions::default(),
                    killed: false,
                    frozen: false,
                    tasks: Vec::new(),
                    task_res_allocator: RecycleAllocator::new(),
                    mutex_list: Vec::new(),
                    semaphore_list: Vec::new(),
                    condvar_list: Vec::new(),
                })
            },
        });
        // create a main thread, we should allocate ustack and trap_cx here
        let task = Arc::new(TaskControlBlock::new(
            Arc::clone(&process),
            ustack_base,
            true,
        ));

        // prepare TrapContext of main thread
        let task_inner = task.inner_exclusive_access();
        let trap_cx = task_inner.get_trap_cx();
        let ustack_top = task_inner.res.as_ref().unwrap().ustack_top(); //用户态该进程(其实是N个线程栈加起来)的栈顶
        let kstack_top = task.kstack.get_top(); //内核中给该进程分配的栈(其实是N个线程栈加起来)的栈顶
        drop(task_inner);
        *trap_cx = TrapContext::app_init_context(
            entry_point,
            ustack_top,
            KERNEL_SPACE.exclusive_access().token(),
            kstack_top,
            trap_handler as usize,
        );
        // add main thread to the process
        let mut process_inner = process.inner_exclusive_access(); //lock process inner
        process_inner.tasks.push(Some(Arc::clone(&task))); // task 就是 thread
        drop(process_inner); //unlock process inner
        insert_into_pid2process(process.getpid(), Arc::clone(&process));
        // add main thread to scheduler
        add_task(task);

        log!("App name : init_proc , Process id : [{}]", &process.pid.0);
        return process;
    }

    /// Only support processes with a single thread.
    pub fn exec(self: &Arc<Self>, app_name: &str, elf_data: &[u8], args: Vec<String>) {
        assert_eq!(self.inner_exclusive_access().thread_count(), 1);
        // memory_set with elf program headers/trampoline/trap context/user stack
        let (memory_set, ustack_base, entry_point) = MemorySet::from_elf(elf_data);
        let new_token = memory_set.token();
        // substitute memory_set
        self.inner_exclusive_access().memory_set = memory_set;
        // then we alloc user resource for main thread again
        // since memory_set has been changed
        let task = self.inner_exclusive_access().get_task(0); //唯一的一个线程 主线程
        let mut task_inner = task.inner_exclusive_access();
        // 重新读取可执行文件后, 栈底 资源 trap上下文都要替换
        task_inner.res.as_mut().unwrap().ustack_base = ustack_base;
        task_inner.res.as_mut().unwrap().alloc_user_res();
        task_inner.trap_cx_ppn = task_inner.res.as_mut().unwrap().trap_cx_ppn();
        self.inner_exclusive_access().app_name = app_name.to_string();
        // push arguments on user stack
        // ex: 传入两个参数 aa和bb
        //                                          argv_base-----|
        //                                |<--8 bytes->|          v   |1B |
        // HighAddr |         0           |  argv[1]   |  argv[0] |\0| a | a |\0| b | b |Alignment|  LowAddr
        //          ^--user_sp (original)      |            |--------------^                      ^---user_sp (now)
        //                                     |--------------------------------------^
        // 1 usize == 8 bytes

        let mut user_sp = task_inner.res.as_mut().unwrap().ustack_top();
        log!(
            "\x1b[32m[SYSCALL : exec] original user_sp : [{}] \x1b[0m",
            user_sp
        );
        user_sp -= (args.len() + 1) * core::mem::size_of::<usize>(); // 先把user_sp栈顶指针下压到 arg[0] 处 , 这里压3个usize 0+arg[1]+arg[0]
        let argv_base = user_sp;
        let mut argv: Vec<_> = (0..=args.len())
            .map(|arg| {
                //ex: argv[0] 的值是入参字符串头字符指针.
                translated_refmut(
                    new_token,
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
                *translated_refmut(new_token, p as *mut u8) = *c;
                p += 1; //下一个字节位置
            }

            *translated_refmut(new_token, p as *mut u8) = 0; //把\0压入栈里,方便应用知道哪里是字符串结尾
            log!(
                "\x1b[32m[SYSCALL : exec] argv[{}] point [{}] ==> string [{}]    \x1b[0m",
                i,
                argv[i],
                translated_str(new_token, *argv[i] as *const u8)
            );
        }
        log!(
            "\x1b[32m[SYSCALL : exec] user stack argv array -- [{:?}] \x1b[0m",
            argv
        );
        // make the user_sp aligned to 8B for k210 platform
        user_sp -= user_sp % core::mem::size_of::<usize>(); //K210平台上访问用户栈会触发访存不对齐的异常 也就是Alignment这块地址
                                                            // initialize trap_cx
        let mut trap_cx = TrapContext::app_init_context(
            entry_point,
            user_sp,
            KERNEL_SPACE.exclusive_access().token(),
            task.kstack.get_top(),
            trap_handler as usize,
        );
        trap_cx.x[10] = args.len(); // x10(a0)寄存器, 函数入参1寄存器 传入命令行参数个数
        trap_cx.x[11] = argv_base; // x11(a1)寄存器, 函数入参2寄存器 传如argv_base 具体位置看上面注释图 读取argv[0]/argv[1]/argv[2]
        *task_inner.get_trap_cx() = trap_cx;

        log!(
            "\x1b[32m[SYSCALL : exec] now user_sp : [{}] \x1b[0m",
            user_sp
        );
        log!("[KERNEL] EXEC Process id : [{}] App name : [{}]", &self.getpid(), &self.inner.exclusive_access().app_name);
    }

    /// Only support process with a single thread.
    pub fn fork(self: &Arc<Self>) -> Arc<Self> {
        let mut parent = self.inner_exclusive_access();
        assert_eq!(parent.thread_count(), 1);
        // copy user space(include trap context)
        let memory_set = MemorySet::from_existed_user(&parent.memory_set);
        // alloc a pid and a kernel stack in kernel space
        let pid = pid_alloc();
        // copy fd table
        let mut new_fd_table: Vec<Option<Arc<dyn File + Send + Sync>>> = Vec::new();
        for fd in parent.fd_table.iter() {
            if let Some(file) = fd {
                new_fd_table.push(Some(file.clone()));
            } else {
                new_fd_table.push(None);
            }
        }
        let working_dir = parent.working_dir.clone();
        let child = Arc::new(Self {
            pid,
            inner: unsafe {
                UPSafeCell::new(ProcessControlBlockInner {
                    is_zombie: false,
                    memory_set,
                    parent: Some(Arc::downgrade(self)),
                    children: Vec::new(),
                    exit_code: 0,
                    fd_table: new_fd_table,
                    signals: SignalFlags::empty(),
                    app_name: String::new(),
                    working_dir,
                    // inherit the signal_mask and signal_action
                    signal_mask: parent.signal_mask,
                    handling_sig: -1,
                    signal_actions: parent.signal_actions.clone(),
                    killed: false,
                    frozen: false,
                    tasks: Vec::new(),
                    task_res_allocator: RecycleAllocator::new(),
                    mutex_list: Vec::new(), //必须置空, 因为fork时候多线程只保留唯一一个线程. 其他线程如果持有锁. 那个锁就再也没人去解了.
                    semaphore_list: Vec::new(), //同上
                    condvar_list: Vec::new(), //同上
                })
            },
        });
        // add child
        parent.children.push(Arc::clone(&child));
        // create main thread of child process
        let task = Arc::new(TaskControlBlock::new(
            Arc::clone(&child),
            parent
                .get_task(0)
                .inner_exclusive_access()
                .res
                .as_ref()
                .unwrap()
                .ustack_base(),
            // here we do not allocate trap_cx or ustack again
            // but mention that we allocate a new kstack here
            false,
        ));
        // attach task (thread) to child process
        let mut child_inner = child.inner_exclusive_access();
        child_inner.tasks.push(Some(Arc::clone(&task)));
        drop(child_inner);
        // modify kernel_sp (kstack_top) in trap_cx of this thread
        let task_inner = task.inner_exclusive_access();
        let trap_cx = task_inner.get_trap_cx();
        trap_cx.kernel_sp = task.kstack.get_top();
        drop(task_inner);
        insert_into_pid2process(child.getpid(), Arc::clone(&child));

        // **** access children PCB exclusively
        log!("[KERNEL] Fork parent Process id : [{}]", &self.getpid());
        log!("[KERNEL] Fork new process id : [{}]", &child.getpid());
        //add this thread to scheduler
        add_task(task);
        return child;
        // ---- release parent PCB automatically
        // **** release children PCB automatically
    }

    pub fn getpid(&self) -> usize {
        self.pid.0
    }

    pub fn get_working_dir(&self) -> String {
        self.inner.exclusive_access().working_dir.clone()
    }
}
