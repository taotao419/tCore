use core::any::Any;

use crate::sync::{Condvar, UPIntrFreeCell};
use crate::task::schedule;
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use virtio_drivers::{VirtIOHeader, VirtIOInput};

use super::virtio::VirtioHal;

const VIRTIO5: usize = 0x10005000;
const VIRTIO6: usize = 0x10006000;

struct VirtIOInputInner {
    // 核心驱动结构体 VirtIOInput
    virtio_input: VirtIOInput<'static, VirtioHal>,
    // InputEvent 结构体变形成一个u64数字, 那么多个InputEvents 就是u64数组. 这里的events就是未处理的事件
    events: VecDeque<u64>,
}

struct VirtIOInputWrapper {
    inner: UPIntrFreeCell<VirtIOInputInner>,
    // 条件变量 使用它可以在没有事件到来的时候 休眠call read_event的线程/进程. 等到IO事件到来时 立即唤醒.
    // 比忙等待强
    condvar: Condvar, 
}

pub trait InputDevice: Send + Sync + Any {
    /// 读一个输入IO的事件 , 可能是鼠标移动 键盘按压按钮 并把返回的InputEvent结构体 压缩成一个u64的数字
    fn read_event(&self) -> u64;
    /// IO设备发出中断请求 比如设备发送鼠标移动事件 需要处理此中断 (可以理解为一个回调函数)
    fn handle_irq(&self);
    /// 是否有IO事件 未处理
    fn is_empty(&self) -> bool;
}

lazy_static::lazy_static!(
    // 固定位置 键盘 0x10005000
    // 固定位置 鼠标 0x10006000 
    pub static ref KEYBOARD_DEVICE: Arc<dyn InputDevice> = Arc::new(VirtIOInputWrapper::new(VIRTIO5));
    pub static ref MOUSE_DEVICE: Arc<dyn InputDevice> = Arc::new(VirtIOInputWrapper::new(VIRTIO6));
);

impl VirtIOInputWrapper {
    pub fn new(addr: usize) -> Self {
        let inner = VirtIOInputInner {
            virtio_input: unsafe {
                VirtIOInput::<VirtioHal>::new(&mut *(addr as *mut VirtIOHeader)).unwrap()
            },
            events: VecDeque::new(),
        };
        return Self {
            inner: unsafe { UPIntrFreeCell::new(inner) },
            condvar: Condvar::new(),
        };
    }
}

impl InputDevice for VirtIOInputWrapper {
    fn is_empty(&self) -> bool {
        // 上锁 -> 看一下事件队列是否为空 -> 解锁
        return self.inner.exclusive_access().events.is_empty();
    }

    fn read_event(&self) -> u64 {
        loop {
            let mut inner = self.inner.exclusive_access(); //上锁
            if let Some(event) = inner.events.pop_front() {
                return event;
            } else {
                // 如果没有IO事件发生 则休眠相应线程/ 进程
                let task_cx_ptr = self.condvar.wait_no_sched();
                drop(inner); //解锁
                schedule(task_cx_ptr);
            }
        }
    }

    fn handle_irq(&self) {
        let mut count = 0;
        let mut result = 0;
        self.inner.exclusive_session(|inner| {
            inner.virtio_input.ack_interrupt();
            // 通过IO驱动 把所有未处理的IO事件全部变成u64的数字 放入events 向量里
            while let Some(event) = inner.virtio_input.pop_pending_event() {
                count += 1;
                result = (event.1.event_type as u64) << 48
                    | (event.1.code as u64) << 32
                    | (event.1.value) as u64;
                inner.events.push_back(result);
            }
        });
        if count > 0 {
            // 如果有未处理IO事件, 唤醒相应线程/ 进程
            self.condvar.signal();
        }
    }
}
