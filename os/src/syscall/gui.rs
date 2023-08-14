use crate::drivers::gpu::GPU_DEVICE;
use crate::mm::{MapArea, MapPermission, MapType, PhysAddr, SectionType, VirtAddr};
use crate::task::current_process;

const FB_VADDR: usize = 0x10000000; // 显存的用户态虚拟内存地址 反正每个用户进程它的显存地址 都是0x10000000

pub fn sys_framebuffer() -> isize {
    let fb = GPU_DEVICE.get_framebuffer(); //显存物理地址
    let len = fb.len();

    let fb_start_pa = PhysAddr::from(fb.as_ptr() as usize);
    assert!(fb_start_pa.aligned());
    let fb_start_ppn = fb_start_pa.floor(); // 物理页
    let fb_start_vpn = VirtAddr::from(FB_VADDR).floor(); // 虚拟页
    let pn_offset = fb_start_ppn.0 as isize - fb_start_vpn.0 as isize;

    let current_process = current_process();
    let mut inner = current_process.inner_exclusive_access();
    inner.memory_set.push(
        MapArea::new(
            (FB_VADDR as usize).into(),
            (FB_VADDR + len as usize).into(),
            MapType::Linear(pn_offset),  //就是虚拟地址 加上某个固定值offset 就是物理地址
            MapPermission::R | MapPermission::W | MapPermission::U, // User Mode + Read + Write
            SectionType::Device,
        ),
        None,
    );
    return FB_VADDR as isize;
}

pub fn sys_framebuffer_flush() -> isize {
    GPU_DEVICE.flush();
    return 0;
}
