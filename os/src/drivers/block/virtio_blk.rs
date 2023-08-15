use super::BlockDevice;
use crate::drivers::bus::virtio::VirtioHal;
use crate::sync::{Condvar, UPIntrFreeCell};
use crate::task::schedule;
use crate::DEV_NON_BLOCKING_ACCESS;
use alloc::collections::BTreeMap;
use virtio_drivers::{BlkResp, Hal, RespStatus, VirtIOBlk, VirtIOHeader};

const VIRTIO0: usize = 0x10008000;

pub struct VirtIOBlock {
    virtio_blk: UPIntrFreeCell<VirtIOBlk<'static, VirtioHal>>,
    condvars: BTreeMap<u16, Condvar>,
}

impl BlockDevice for VirtIOBlock {
    fn read_block(&self, block_id: usize, buf: &mut [u8]) {
        let nb = *DEV_NON_BLOCKING_ACCESS.exclusive_access(); //全局开关 : 是否阻塞
        if nb {
            // 非阻塞模式 (Aka 中断方式)
            let mut resp = BlkResp::default();
            let task_cx_ptr = self.virtio_blk.exclusive_session(|blk| {
                let token = unsafe { blk.read_block_nb(block_id, buf, &mut resp).unwrap() }; // 这里的token 就是Descriptor链的头元素id
                self.condvars.get(&token).unwrap().wait_no_sched() //将当前线程/进程 加入条件变量的等待队列
            });
            // log!("\x1b[38;5;208m[BLOCK DRIVE: read_block] block id: [{}] , sleep current thread/process  \x1b[0m",block_id);
            schedule(task_cx_ptr); // 此线程/进程 进入休眠. 直到驱动取出数据 通过条件变量唤醒此线程/进程
            assert_eq!(
                resp.status(),
                RespStatus::Ok,
                "Error when reading VirtIOBlk"
            );
        } else {
            // 阻塞模式 (Aka 轮询模式)
            self.virtio_blk
                .exclusive_access()
                .read_block(block_id, buf)
                .expect("Error when reading VirtIOBlk");
        }
    }

    fn write_block(&self, block_id: usize, buf: &[u8]) {
        let nb = *DEV_NON_BLOCKING_ACCESS.exclusive_access(); //全局开关 : 是否阻塞
        if nb {
            // 非阻塞模式 (Aka 中断方式)
            let mut resp = BlkResp::default();
            let task_cx_ptr = self.virtio_blk.exclusive_session(|blk| {
                let token = unsafe { blk.write_block_nb(block_id, buf, &mut resp).unwrap() }; // 这里的token 就是Descriptor链的头元素id
                self.condvars.get(&token).unwrap().wait_no_sched()
            });
            schedule(task_cx_ptr); // 此线程/进程 进入休眠. 直到驱动取出数据 通过条件变量唤醒此线程/进程
            assert_eq!(
                resp.status(),
                RespStatus::Ok,
                "Error when writing VirtIOBlk"
            );
        } else {
            self.virtio_blk
                .exclusive_access()
                .write_block(block_id, buf)
                .expect("Error when writing VirtIOBlk");
        }
    }

    fn handle_irq(&self) {
        self.virtio_blk.exclusive_session(|blk| {
            while let Ok(token) = blk.pop_used() {
                // 唤醒等待该块设备I/O完成的线程/进程
                // log!( "\x1b[35m[BLOCK DRIVE: handle_irq] token [{}]  \x1b[0m", token);
                self.condvars.get(&token).unwrap().signal();
            }
        });
    }
}

impl VirtIOBlock {
    pub fn new() -> Self {
        let virtio_blk = unsafe {
            UPIntrFreeCell::new(
                VirtIOBlk::<VirtioHal>::new(&mut *(VIRTIO0 as *mut VirtIOHeader)).unwrap(),
            )
        };
        let mut condvars = BTreeMap::new();
        let channels = virtio_blk.exclusive_access().virt_queue_size();
        for i in 0..channels {
            let condvar = Condvar::new();
            condvars.insert(i, condvar);
        }
        return Self {
            virtio_blk,
            condvars,
        };
    }
}
