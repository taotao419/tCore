use super::CharDevice;
use crate::sync::{Condvar, UPIntrFreeCell};
use crate::task::schedule;
use alloc::collections::VecDeque;
use bitflags::*;
use volatile::{ReadOnly, Volatile, WriteOnly};

bitflags! {
    // Interrupt Enable Register
    pub struct IER: u8{
        const RX_AVAILABLE = 1 << 0; // Bit 0: Received data available 有可读数据了
        const TX_EMPTY = 1 << 1; // Bit 1: Transmitter holding register empty 清空写的寄存器
    }

    // Line Status Register 线路状态寄存器显示通信的当前状态
    pub struct LSR : u8{
        const DATA_AVAILABLE = 1 << 0; // Bit 0: Data available
        const THR_EMPTY = 1 << 5; // Bit 5: THR is empty
    }

    // Modem Control Register 用于对连接的设备执行握手操作
    pub struct MCR: u8{
        const DATA_TERMINAL_READY = 1 << 0; // Data terminal ready
        const REQUEST_TO_SEND = 1 << 1; // Request to send
        const AUX_OUTPUT1 = 1 << 2; // Auxiliary output 1
        const AUX_OUTPUT2 = 1 << 3; // Auxiliary output 2
    }
}

#[repr(C)]
#[allow(dead_code)]
struct ReadWithoutDLAB {
    /// receiver buffer register
    pub rbr: ReadOnly<u8>,
    /// interrupt enable register
    pub ier: Volatile<IER>,
    /// interrupt identification register
    pub iir: ReadOnly<u8>,
    /// line control register
    pub lcr: Volatile<u8>,
    /// modem control register
    pub mcr: Volatile<MCR>,
    /// line status register
    pub lsr: ReadOnly<LSR>,
    /// ignore MSR
    _padding1: ReadOnly<u8>,
    /// ignore SCR
    _padding2: ReadOnly<u8>,
}

#[repr(C)]
#[allow(dead_code)]
struct WriteWithoutDLAB {
    /// transmitter holding register
    pub thr: WriteOnly<u8>,
    /// interrupt enable register
    pub ier: Volatile<IER>,
    /// ignore FCR
    _padding0: ReadOnly<u8>,
    /// line control register
    pub lcr: Volatile<u8>,
    /// modem control register
    pub mcr: Volatile<MCR>,
    /// line status register
    pub lsr: ReadOnly<LSR>,
    /// ignore other registers
    _padding1: ReadOnly<u16>,
}

pub struct NS16550aRaw {
    base_addr: usize,
}

impl NS16550aRaw {
    // 读口 把8个 byte 一口气读出来作为一个ReadWithoutDLAB 结构体
    fn read_end(&mut self) -> &mut ReadWithoutDLAB {
        unsafe { &mut *(self.base_addr as *mut ReadWithoutDLAB) }
    }

    // 写口 把8个 byte 一口气读出来作为一个WriteWithoutDLAB 结构体
    fn write_end(&mut self) -> &mut WriteWithoutDLAB {
        unsafe { &mut *(self.base_addr as *mut WriteWithoutDLAB) }
    }

    pub fn new(base_addr: usize) -> Self {
        Self { base_addr }
    }

    pub fn init(&mut self) {
        let read_end = self.read_end();
        // 初始化 Modem control register 用于连接的社保执行握手操作
        // 包括在16550在内的原始UART系列中, 控制信号的设置和重置必须通过软件完成.
        let mut mcr = MCR::empty();
        mcr |= MCR::DATA_TERMINAL_READY;
        mcr |= MCR::REQUEST_TO_SEND;
        mcr |= MCR::AUX_OUTPUT2; // Auxiliary output 2
        read_end.mcr.write(mcr);

        let ier = IER::RX_AVAILABLE; // Received data available. 打开接收数据中断开关
        read_end.ier.write(ier);
    }

    pub fn read(&mut self) -> Option<u8> {
        let read_end = self.read_end();
        let lsr = read_end.lsr.read();
        // 检查线路状态寄存器 第一位bit 表示是否有值
        if lsr.contains(LSR::DATA_AVAILABLE) {
            // 读一个byte 从接收缓存寄存器(RBR)
            return Some(read_end.rbr.read());
        } else {
            return None;
        }
    }

    pub fn write(&mut self, ch: u8) {
        let write_end = self.write_end();
        loop {
            // 从LSR 状态寄存器看看 是否THR 写寄存器为空, 允许写入一个byte
            if write_end.lsr.read().contains(LSR::THR_EMPTY) {
                // 往 写寄存器(THR)  写入一byte
                write_end.thr.write(ch);
                break;
            }
        }
    }
}

struct NS16550aInner {
    ns16550a: NS16550aRaw,
    read_buffer: VecDeque<u8>,
}

pub struct NS16550a<const BASE_ADDR: usize> {
    inner: UPIntrFreeCell<NS16550aInner>,
    condvar: Condvar,
}

impl<const BASE_ADDR: usize> NS16550a<BASE_ADDR> {
    pub fn new() -> Self {
        let inner = NS16550aInner {
            ns16550a: NS16550aRaw::new(BASE_ADDR),
            read_buffer: VecDeque::new(),
        };

        Self {
            inner: unsafe { UPIntrFreeCell::new(inner) },
            condvar: Condvar::new(),
        }
    }

    pub fn read_buffer_is_emptyu(&self) -> bool {
        return self
            .inner
            .exclusive_session(|inner| inner.read_buffer.is_empty());
    }
}

impl<const BASE_ADDR: usize> CharDevice for NS16550a<BASE_ADDR> {
    fn init(&self) {
        let mut inner = self.inner.exclusive_access(); // lock & acquire
        inner.ns16550a.init();
        drop(inner); // unlock
    }

    fn read(&self) -> u8 {
        loop {
            let mut inner = self.inner.exclusive_access(); // lock and acquire
            if let Some(ch) = inner.read_buffer.pop_front() {
                return ch;
            } else {
                let task_cx_ptr = self.condvar.wait_no_sched(); // 没接受到数据则阻塞
                drop(inner);
                schedule(task_cx_ptr); // 走到这步势必已经又获取到数据 需要重新安排CPU时间片(资源)
            }
        }
    }
    // write 只是包了一层 上锁功能.
    fn write(&self, ch: u8) {
        let mut inner = self.inner.exclusive_access();
        inner.ns16550a.write(ch);
    }

    fn handle_irq(&self) {
        let mut count = 0;
        self.inner.exclusive_session(|inner| {
            while let Some(ch) = inner.ns16550a.read() {
                count += 1;
                inner.read_buffer.push_back(ch); //能读的话尽量读完, 并放在缓冲区里
            }
        });
        if count > 0 {
            self.condvar.signal(); //配合上面的read() 函数来让read()函数继续执行.  let task_cx_ptr = self.condvar.wait_no_sched();  这行
        }
    }
}
