use crate::mm::{
    frame_alloc, frame_dealloc, kernel_token, FrameTracker, PageTable, PhysAddr,
    PhysPageNum, StepByOne, VirtAddr,
};
use crate::sync::UPIntrFreeCell;
use alloc::vec::Vec;
use lazy_static::*;
use virtio_drivers::Hal;

lazy_static! {
    // 貌似现在并没有代码引用QUEUE_FRAMES
    static ref QUEUE_FRAMES: UPIntrFreeCell<Vec<FrameTracker>> = unsafe{
        UPIntrFreeCell::new(Vec::new())
    };
}

pub struct VirtioHal;

impl Hal for VirtioHal {
    fn dma_alloc(pages: usize) -> usize {
        let mut ppn_base = PhysPageNum(0);
        for i in 0..pages {
            let frame = frame_alloc().unwrap();
            if i == 0 {
                ppn_base = frame.ppn;
            }
            assert_eq!(frame.ppn.0, ppn_base.0 + i);
            QUEUE_FRAMES.exclusive_access().push(frame);
        }
        let pa: PhysAddr = ppn_base.into(); //物理页帧 到 物理地址的转换
        return pa.0;
    }

    fn dma_dealloc(paddr: usize, pages: usize) -> i32 {
        let pa = PhysAddr::from(paddr); //u64数字 转为 物理地址 (其实啥都没干)
        let mut ppn_base: PhysPageNum = pa.into(); // 物理地址 转成 物理页帧
        for _ in 0..pages {
            frame_dealloc(ppn_base); //循环释放物理页帧的内存
            ppn_base.step(); // pageNum ++
        }
        return 0;
    }

    fn phys_to_virt(paddr: usize) -> usize {
        return paddr;
    }

    fn virt_to_phys(vaddr: usize) -> usize {
        return PageTable::from_token(kernel_token())
            .translate_va(VirtAddr::from(vaddr))
            .unwrap()
            .0;
    }
}
