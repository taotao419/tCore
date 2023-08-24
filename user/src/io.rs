use embedded_graphics::{
    pixelcolor::Rgb888,
    prelude::{DrawTarget, OriginDimensions, RgbColor, Size},
    Pixel,
};
use virtio_input_decoder::Decoder;
pub use virtio_input_decoder::{DecodeType, Key, KeyType, Mouse};

use crate::console::log;
use crate::syscall::{sys_event_get, sys_framebuffer, sys_framebuffer_flush, sys_key_pressed};
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
            fb_ptr as usize, VIRTGPU_LEN
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

pub fn event_get() -> Option<InputEvent> {
    let raw_value = sys_event_get();
    if raw_value == 0 {
        None
    } else {
        Some((raw_value as u64).into())
    }
}

pub fn key_pressed() -> bool {
    if sys_key_pressed() == 1 {
        true
    } else {
        false
    }
}

#[repr(C)]
pub struct InputEvent {
    pub event_type: u16,
    pub code: u16,
    pub value: u32,
}

impl From<u64> for InputEvent {
    fn from(mut v: u64) -> Self {
        let value = v as u32;
        v >>= 32;
        let code = v as u16;
        v >>= 16;
        let event_type = v as u16;
        Self {
            event_type,
            code,
            value,
        }
    }
}

impl InputEvent {
    pub fn decode(&self) -> Option<DecodeType> {
        Decoder::decode(
            self.event_type as usize,
            self.code as usize,
            self.value as usize,
        )
        .ok()
    }
}
