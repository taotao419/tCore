use alloc::sync::Arc;

use crate::sync::{Condvar, Mutex, MutexBlocking, MutexSpin, Semaphore};
use crate::task::{block_current_and_run_next, current_process, current_task};
use crate::timer::{add_timer, get_time_ms};

pub fn sys_sleep(ms: usize) -> isize {
    let expire_ms = get_time_ms() + ms;
    let task = current_task().unwrap();
    add_timer(expire_ms, task);
    block_current_and_run_next();
    return 0;
}

pub fn sys_mutex_create(blocking: bool) -> isize {
    let process = current_process();
    let mutex: Option<Arc<dyn Mutex>> = if !blocking {
        Some(Arc::new(MutexSpin::new())) //实现方式1 自旋锁
    } else {
        Some(Arc::new(MutexBlocking::new())) //实现方式2 阻塞
    };
    let mut process_inner = process.inner_exclusive_access();
    if let Some(id) = process_inner
        .mutex_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.mutex_list[id] = mutex;
        id as isize
    } else {
        process_inner.mutex_list.push(mutex);
        process_inner.mutex_list.len() as isize - 1
    }
}

pub fn sys_mutex_lock(mutex_id: usize) -> isize {
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    drop(process_inner);
    drop(process);
    mutex.lock(); //核心就这么一句话, 根据mutex_id 取出 mutex , 执行下 lock 方法
    return 0;
}

pub fn sys_mutex_unlock(mutex_id: usize) -> isize {
    let process = current_process();
    let process_inner = process.inner_exclusive_access();
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    drop(process_inner);
    drop(process);
    mutex.unlock(); //核心就这么一句话, 根据mutex_id 取出 mutex , 执行下 unlock 方法
    return 0;
}

pub fn sys_semaphore_create(res_count: usize) -> isize {
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    // 通用逻辑, 如果这个列表中挑出第一个空巢 empty slot, 返回这个空巢的id
    // 如果列表全部是满的, 那么在尾部 append 一个元素 , 列表长度作为id 返回
    let id = if let Some(id) = process_inner
        .semaphore_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.semaphore_list[id] = Some(Arc::new(Semaphore::new(res_count)));
        id
    } else {
        process_inner
            .semaphore_list
            .push(Some(Arc::new(Semaphore::new(res_count))));
        process_inner.semaphore_list.len() - 1
    };

    return id as isize;
}

pub fn sys_semaphore_up(sem_id: usize) -> isize {
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let sem = Arc::clone(process_inner.semaphore_list[sem_id].as_ref().unwrap());
    drop(process_inner);
    sem.up();
    return 0;
}

pub fn sys_semaphore_down(sem_id: usize) -> isize {
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let sem = Arc::clone(process_inner.semaphore_list[sem_id].as_ref().unwrap());
    drop(process_inner);
    sem.down();
    return 0;
}

pub fn sys_condvar_create() -> isize {
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    // 通用逻辑, 如果这个列表中挑出第一个空巢 empty slot, 返回这个空巢的id
    // 如果列表全部是满的, 那么在尾部 append 一个元素 , 列表长度作为id 返回
    let id = if let Some(id) = process_inner
        .condvar_list
        .iter()
        .enumerate()
        .find(|(_, item)| item.is_none())
        .map(|(id, _)| id)
    {
        process_inner.condvar_list[id] = Some(Arc::new(Condvar::new()));
        id
    } else {
        process_inner
            .condvar_list
            .push(Some(Arc::new(Condvar::new())));
        process_inner.condvar_list.len() - 1
    };
    log!("\x1b[38;5;208m[SYSCALL : Condvar] 条件变量创建 id [{}]  \x1b[0m",id);
    return id as isize;
}

pub fn sys_condvar_signal(condvar_id: usize) -> isize {
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let condvar = Arc::clone(process_inner.condvar_list[condvar_id].as_ref().unwrap());
    drop(process_inner);
    condvar.signal();
    log!("\x1b[38;5;208m[SYSCALL : Condvar] 条件变量通知 id [{}]  \x1b[0m",condvar_id);
    return 0;
}

pub fn sys_condvar_wait(condvar_id: usize, mutex_id: usize) -> isize {
    let process = current_process();
    let mut process_inner = process.inner_exclusive_access();
    let condvar = Arc::clone(process_inner.condvar_list[condvar_id].as_ref().unwrap());
    let mutex = Arc::clone(process_inner.mutex_list[mutex_id].as_ref().unwrap());
    drop(process_inner);
    log!("\x1b[38;5;208m[SYSCALL : Condvar] 条件变量等待 condvar_id [{}] , mutex_id [{}]  \x1b[0m",condvar_id, mutex_id);
    condvar.wait(mutex);
    return 0;
}
