use embedded_graphics::prelude::{RgbColor, Size};

use crate::syscall::{sys_framebuffer, sys_framebuffer_flush};

pub const VIRTGPU_XRES: u32 = 1280;
pub const VIRTGPU_YRES: u32 = 800;
pub const VIRTGPU_LEN: usize = (VIRTGPU_XRES * VIRTGPU_YRES * 4) as usize;

pub fn framebuffer() -> isize {
    return sys_framebuffer();
}

pub fn framebuffer_flush() -> isize {
    return sys_framebuffer_flush();
}

pub struct Display {
    pub size: Size,
    pub fb: &'static mut [u8],
}

impl Display {
    pub fn new(size: Size) -> Self {
        let fb_ptr = framebuffer() as *mut u8;
        let fb = unsafe { core::slice::from_raw_parts_mut(fb_ptr, VIRTGPU_LEN) };
        return Self { size, fb };
    }

    pub fn framebuffer(&mut self)->&mut [u8]{
        self.fb
    }

    ///就是帮你最后调用了下 flush方法.
    pub fn paint_on_framebuffer(&mut self, p: impl FnOnce(&mut [u8])->()){
        p(self.framebuffer());
        framebuffer_flush();
    }
}
