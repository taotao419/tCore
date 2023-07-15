mod ns16550a;

use crate::board::CharDeviceImpl;
use alloc::sync::Arc;
use lazy_static::*;
pub use ns16550a::NS16550a;

pub trait CharDevice {
    fn init(&self); // 初始化
    fn read(&self) -> u8; // 读一个 byte
    fn write(&self, ch: u8); // 写一个 byte
    fn handle_irq(&self); //处理中断
}

lazy_static! {
    pub static ref UART: Arc<CharDeviceImpl> = Arc::new(CharDeviceImpl::new());
}
