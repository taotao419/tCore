#![no_std]
#![no_main]

#[macro_use]
extern crate user_lib;

use embedded_graphics::prelude::Size;
use user_lib::{Display, framebuffer_flush, framebuffer};

pub const VIRTGPU_XRES: usize = 1280;
pub const VIRTGPU_YRES: usize = 800;

// from Wikipedia
fn hsv_to_rgb(h: u32, s: f32, v: f32) -> (f32, f32, f32) {
    let hi = (h / 60) % 6;
    let f = (h % 60) as f32 / 60.0;
    let p = v * (1.0 - s);
    let q = v * (1.0 - f * s);
    let t = v * (1.0 - (1.0 - f) * s);
    match hi {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        5 => (v, p, q),
        _ => panic!("error"),
    }
}

#[no_mangle]
pub fn main() -> i32 {
    println!("start gui_mandelbrot app");
    let fb_ptr =framebuffer() as *mut u8;
    println!("call framebuffer");
    let fb= unsafe {core::slice::from_raw_parts_mut(fb_ptr as *mut u8, VIRTGPU_XRES*VIRTGPU_YRES*4 as usize)};
    println!("call from_raw_parts_mut");
    let width = VIRTGPU_XRES as usize;
    let height = VIRTGPU_YRES as usize;
    for y in 0..800 {
        println!("loop y : {}",y);
        for x in 0..1280 {
            let idx = (y * width + x) * 4;
            let scale = 5e-3;
            let xx = (x as f32 - width as f32 / 2.0) * scale;
            let yy = (y as f32 - height as f32 / 2.0) * scale;
            let mut re = xx as f32;
            let mut im = yy as f32;
            let mut iter: u32 = 0;
            loop {
                iter = iter + 1;
                let new_re = re * re - im * im + xx as f32;
                let new_im = re * im * 2.0 + yy as f32;
                if new_re * new_re + new_im * new_im > 1e3 {
                    break;
                }
                re = new_re;
                im = new_im;

                if iter > 5 {
                    break;
                }
            }
            iter = iter * 6;
            let (r, g, b) = hsv_to_rgb(iter, 1.0, 0.5);
            let rr = (r * 256.0) as u32;
            let gg = (g * 256.0) as u32;
            let bb = (b * 256.0) as u32;
            // let color = (bb << 16) | (gg << 8) | rr;
            fb[idx] = bb as u8; // Blue
            fb[idx + 1] = gg as u8; // Green
            fb[idx + 2] = rr as u8; //Red
        }
        
    }
    framebuffer_flush();
    0
}


