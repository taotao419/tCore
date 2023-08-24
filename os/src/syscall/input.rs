use crate::drivers::{KEYBOARD_DEVICE, MOUSE_DEVICE, UART};

/// 系统调用 读取键盘与鼠标的 IO事件. 如果没有读到返回0
pub fn sys_event_get() -> isize {
    let kb = KEYBOARD_DEVICE.clone();
    let mouse = MOUSE_DEVICE.clone();

    if !kb.is_empty() {
        return kb.read_event() as isize;
    } else if !mouse.is_empty() {
        return mouse.read_event() as isize;
    } else {
        return 0;
    }
}

pub fn sys_key_pressed() -> isize {
    let res = !UART.read_buffer_is_empty();
    if res {
        return 1;
    } else {
        return 0;
    }
}
