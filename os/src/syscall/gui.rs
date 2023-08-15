use crate::drivers::gpu::GPU_DEVICE;
use crate::mm::{MapArea, MapPermission, MapType, PPNRange, PhysAddr, SectionType, VirtAddr};
use crate::task::current_process;

const FB_VADDR: usize = 0x10000000; // 显存的用户态虚拟内存地址 反正每个用户进程它的显存地址 都是0x10000000

pub fn sys_framebuffer() -> isize {
    let gpu = GPU_DEVICE.clone();
    let fb = gpu.get_framebuffer(); //显存物理地址
    let len = fb.len();
    println!(
        "[kernel] FrameBuffer: addr 0x{:X}, len {}",
        fb.as_ptr() as usize,
        len
    );
    let fb_ppn = PhysAddr::from(fb.as_ptr() as usize).floor();
    let fb_end_ppn = PhysAddr::from(fb.as_ptr() as usize + len).ceil();

    let current_process = current_process();
    let mut inner = current_process.inner_exclusive_access();
    let mem_set = &mut inner.memory_set;
    mem_set.push_noalloc(
        MapArea::new(
            (FB_VADDR as usize).into(),
            (FB_VADDR + len as usize).into(),
            MapType::Noalloc,
            MapPermission::R | MapPermission::W | MapPermission::U, // User Mode + Read + Write
            SectionType::Device,
        ),
        PPNRange::new(fb_ppn, fb_end_ppn),
    );
    return FB_VADDR as isize;
}

pub fn sys_framebuffer_flush() -> isize {
    GPU_DEVICE.flush();
    return 0;
}
