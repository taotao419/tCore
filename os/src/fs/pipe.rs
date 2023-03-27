use super::File;
use crate::mm::UserBuffer;
use crate::sync::UPSafeCell;
use alloc::sync::{Arc, Weak};

use crate::task::suspend_current_and_run_next;


pub struct Pipe {
    readable: bool,
    writable: bool,
    buffer: Arc<UPSafeCell<PipeRingBuffer>>,
}

impl Pipe {
    pub fn read_end_with_buffer(buffer: Arc<UPSafeCell<PipeRingBuffer>>) -> Self {
        Self {
            readable: true,
            writable: false,
            buffer,
        }
    }

    pub fn write_end_with_buffer(buffer: Arc<UPSafeCell<PipeRingBuffer>>) -> Self {
        Self {
            readable: false,
            writable: true,
            buffer,
        }
    }
}

const RING_BUFFER_SIZE: usize = 32;

#[derive(Copy, Clone, PartialEq)]
enum RingBufferStatus {
    Full,
    Empty,
    Normal,
}

pub struct PipeRingBuffer {
    arr: [u8; RING_BUFFER_SIZE],
    head: usize,
    tail: usize,
    status: RingBufferStatus,
    write_end: Option<Weak<Pipe>>,
}

impl PipeRingBuffer {
    pub fn new() -> Self {
        Self {
            arr: [0; RING_BUFFER_SIZE],
            head: 0,
            tail: 0,
            status: RingBufferStatus::Empty,
            write_end: None,
        }
    }

    pub fn set_write_end(&mut self, write_end: &Arc<Pipe>) {
        self.write_end = Some(Arc::downgrade(write_end));
    }

    /// 往环状队列缓存里写入一个字节
    pub fn write_byte(&mut self, byte: u8) {
        self.status = RingBufferStatus::Normal;
        self.arr[self.tail] = byte; //核心: 在tail下标处写入这个字节
        self.tail = (self.tail + 1) % RING_BUFFER_SIZE; // tail下标后移一位, 如果超过RING_BUFFER_SIZE, 就移动到0
        if self.tail == self.head {
            self.status = RingBufferStatus::Full; //判断下是否已经写满了
        }
    }
    /// 往环状队列缓存里读出一个字节
    pub fn read_byte(&mut self) -> u8 {
        self.status = RingBufferStatus::Normal;
        let c = self.arr[self.head]; //核心: 在head下标处读出这个字节
        self.head = (self.head + 1) % RING_BUFFER_SIZE; // head下标后移一位, 如果超过RING_BUFFER_SIZE, 就移动到0
        if self.head == self.tail {
            self.status = RingBufferStatus::Empty; //判断下是否已经读空了
        }
        return c;
    }

    pub fn available_read(&self) -> usize {
        if self.status == RingBufferStatus::Empty {
            return 0;
        } else if self.tail > self.head {
            return self.tail - self.head;
        } else {
            return self.tail + RING_BUFFER_SIZE - self.head;
        }
    }

    pub fn available_write(&self) -> usize {
        if self.status == RingBufferStatus::Full {
            return 0; //环状队列满了自然可写的格子为0
        } else {
            return RING_BUFFER_SIZE - self.available_read(); //可写的数量 就是去除可读的数量剩下的
        }
    }

    pub fn all_write_ends_closed(&self) -> bool {
        return self.write_end.as_ref().unwrap().upgrade().is_none();
    }
}

/// Return (read_end, write_end)
pub fn make_pipe() -> (Arc<Pipe>, Arc<Pipe>) {
    let buffer = Arc::new(unsafe { UPSafeCell::new(PipeRingBuffer::new()) });
    let read_end = Arc::new(Pipe::read_end_with_buffer(buffer.clone()));
    let write_end = Arc::new(Pipe::write_end_with_buffer(buffer.clone()));
    buffer.exclusive_access().set_write_end(&write_end);
    return (read_end, write_end);
}

impl File for Pipe {
    fn readable(&self) -> bool {
        return self.readable;
    }

    fn writable(&self) -> bool {
        return self.writable;
    }

    /// *************** 从环状缓存 ==读==到==> 应用缓存
    /// UserBuffer 是磁盘的字节流在内核中对应的一块内存映射, 可以认为是一个磁盘文件的镜像
    fn read(&self, buf: UserBuffer) -> usize {
        assert!(self.readable());
        let want_to_read = buf.len();
        //转换为迭代器, 这个迭代器就是call一次next函数, 吐出一个指向某个字节的指针
        //大致画下应用缓冲区UserBuffer内部结构
        // |a|b|c|d|a|d|a|c|b
        // |a|c|a|a|d|d|b
        // |c|b|a|a|b
        // 是一个Vec<[u8]>类型 一个u8数组的列表,
        // 比如第一次call next(), 返回的不是第一行第一个字节a ,而是指向a的指针
        // 第二次call , 就是指向第一行第二个字节b的指针
        let mut buf_iter = buf.into_iter();
        let mut already_read = 0usize; //维护实际有多少字节从管道读到应用的缓冲区
        loop {
            let mut ring_buffer = self.buffer.exclusive_access();
            let loop_read = ring_buffer.available_read(); //这轮循环中能读取多少字符
            if loop_read == 0 {
                //如果读取字符为零, 可能需要确认下写端是否已经关闭了
                if ring_buffer.all_write_ends_closed() {
                    return already_read; //确认写端关闭, 直接返回
                }
                drop(ring_buffer);
                suspend_current_and_run_next(); // sys_yield, 等下次轮到CPU执行的时候 进入下个循环
                continue;
            }
            //下面部分是读出了管道的字节
            for _ in 0..loop_read {
                //获取了某个字节的指针, 具体看本函数上面那部分的示意注释
                if let Some(byte_ref) = buf_iter.next() {
                    unsafe {
                        //从环状buffer读出一个字节, 找到应用缓冲区某个字节的指针, 指针指向的字节里面的值替换进去
                        *byte_ref = ring_buffer.read_byte();
                    }
                    already_read += 1;
                    if already_read == want_to_read {
                        //说明应用缓冲区都写满了, 直接返回
                        return want_to_read;
                    }
                    //进入下个循环, 读取下个字节
                } else {
                    //call next(), 返回None 进入这里.
                    return already_read;
                }
            }
        }
    }

    fn write(&self, buf: UserBuffer) -> usize {
        assert!(self.writable());
        let want_to_write = buf.len();
        let mut buf_iter = buf.into_iter();
        let mut already_write = 0usize;
        loop {
            let mut ring_buffer = self.buffer.exclusive_access();
            let loop_write = ring_buffer.available_write();
            if loop_write == 0 {//环状队列满了
                drop(ring_buffer);
                suspend_current_and_run_next();// sys_yield, 等下次轮到CPU执行的时候 进入下个循环
                continue;
            }
            // write at most loop_write bytes
            for _ in 0..loop_write {
                //buf_iter.next()返回的是Option<*mut u8>, 这样byte_ref就是一个拆箱成功后得到的一个指向字节的指针
                if let Some(byte_ref) = buf_iter.next() {
                    ring_buffer.write_byte(unsafe { *byte_ref }); //核心: 从应用缓冲区读到的一个字节(指针指向的一个字节) 写入到环状队列缓存
                    already_write += 1;
                    if already_write == want_to_write {
                        return want_to_write;
                    }
                } else { //拆箱失败直接返回,失败的理由就是迭代器走到缓冲区UserBuffer的最后了.
                    return already_write;
                }
            }
        }
    }
}
