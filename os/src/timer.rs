use core::cmp::Ordering;

use crate::config::CLOCK_FREQ;
use crate::sbi::set_timer;
use crate::sync::UPSafeCell;
use crate::task::{wakeup_task, TaskControlBlock};
use alloc::collections::BinaryHeap;
use alloc::sync::Arc;
use lazy_static::*;
use riscv::register::time;

const TICKS_PER_SEC: usize = 100;
const MSEC_PER_SEC: usize = 1000;

pub fn get_time() -> usize {
    return time::read();
}

pub fn get_time_ms() -> usize {
    time::read() / (CLOCK_FREQ / MSEC_PER_SEC)
}

//every 100ms trigger timer interrupt.
pub fn set_next_trigger() {
    set_timer(get_time() + CLOCK_FREQ / TICKS_PER_SEC);
}

pub struct TimerCondVar {
    pub expire_ms: usize,
    pub task: Arc<TaskControlBlock>,
}
impl PartialEq for TimerCondVar {
    fn eq(&self, other: &Self) -> bool {
        return self.expire_ms == other.expire_ms;
    }
}
impl Eq for TimerCondVar {}
impl PartialOrd for TimerCondVar {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        // 这里用负数的原因是 , 二叉堆默认是最大数排在首位. 而我们要的是时间最近(小)的, 先超时需要先 pop.
        let a = -(self.expire_ms as isize);
        let b = -(other.expire_ms as isize);
        return Some(a.cmp(&b));
    }
}

impl Ord for TimerCondVar {
    fn cmp(&self, other: &Self) -> Ordering {
        return self.partial_cmp(other).unwrap();
    }
}

lazy_static! {
    static ref TIMERS: UPSafeCell<BinaryHeap<TimerCondVar>> =
        unsafe { UPSafeCell::new(BinaryHeap::<TimerCondVar>::new()) };
}

pub fn add_timer(expire_ms: usize, task: Arc<TaskControlBlock>) {
    let mut timers = TIMERS.exclusive_access();
    let tid = task.inner_exclusive_access().res.as_ref().unwrap().tid;
    log!(
        "\x1b[34m[SYSCALL] SLEEP add timer -- sleep thread tid [{}] , wake up timing [{}] ms \x1b[0m",
        tid, expire_ms
    );
    timers.push(TimerCondVar { expire_ms, task });
}

pub fn remove_timer(task: Arc<TaskControlBlock>) {
    let mut timers = TIMERS.exclusive_access();
    let mut temp = BinaryHeap::<TimerCondVar>::new();
    // 用循环的办法, 移除指定的 TCB
    for condvar in timers.drain() {
        if Arc::as_ptr(&task) != Arc::as_ptr(&condvar.task) {
            temp.push(condvar);
        }
    }
    timers.clear();
    timers.append(&mut temp);
}

pub fn check_timer() {
    let current_ms = get_time_ms();
    let mut timers = TIMERS.exclusive_access();
    while let Some(timer) = timers.peek() {
        if timer.expire_ms <= current_ms {
            let tid = timer
                .task
                .inner_exclusive_access()
                .res
                .as_ref()
                .unwrap()
                .tid;
            log!(
                "\x1b[34m[SYSCALL] SLEEP check timer -- wake up thread tid [{}] \x1b[0m",
                tid
            );
            wakeup_task(Arc::clone(&timer.task));
            timers.pop();
        } else {
            break;
        }
    }
}
