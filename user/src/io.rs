use embedded_graphics::{
    pixelcolor::Rgb888,
    prelude::{DrawTarget, OriginDimensions, RgbColor, Size},
    Pixel,
};

use crate::{syscall::{sys_framebuffer, sys_framebuffer_flush}, console::log};

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
        println!(
            "Display Info in user mode program! 0x{:X} , len {}",
            fb_ptr as usize,
            VIRTGPU_LEN
        );
        let fb = unsafe { core::slice::from_raw_parts_mut(fb_ptr, VIRTGPU_LEN) };
        return Self { size, fb };
    }

    pub fn framebuffer(&mut self) -> &mut [u8] {
        self.fb
    }

    ///就是帮你最后调用了下 flush方法.
    pub fn paint_on_framebuffer(&mut self, p: impl FnOnce(&mut [u8]) -> ()) {
        p(self.framebuffer());
        framebuffer_flush();
    }
}

impl OriginDimensions for Display {
    fn size(&self) -> Size {
        return self.size;
    }
}

impl DrawTarget for Display {
    type Color = Rgb888;

    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        pixels.into_iter().for_each(|pixel| {
            let idx = (pixel.0.y * VIRTGPU_XRES as i32 + pixel.0.x) as usize * 4;
            if idx + 2 >= self.fb.len() {
                return;
            }
            self.fb[idx] = pixel.1.b();
            self.fb[idx + 1] = pixel.1.g();
            self.fb[idx + 2] = pixel.1.r();
        });
        framebuffer_flush();
        Ok(())
    }
}
