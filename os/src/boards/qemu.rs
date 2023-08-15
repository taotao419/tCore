pub const CLOCK_FREQ: usize = 12500000;

pub const MMIO: &[(usize, usize)] = &[
    (0x0010_0000, 0x00_2000), // VIRT_TEST/RTC  in virt machine
    (0x2000000, 0x10000),
    (0xc000000, 0x210000), // VIRT_PLIC in virt machine
    (0x10000000, 0x9000),  // VIRT_UART0 with GPU  in virt machine
];

pub type BlockDeviceImpl = crate::drivers::block::VirtIOBlock;
pub type CharDeviceImpl = crate::drivers::chardev::NS16550a<VIRT_UART>;

//下面两个常量, 具体可以从/doc/riscv64-virt.dts 文件里看到
pub const VIRT_PLIC: usize = 0xC00_0000;
pub const VIRT_UART: usize = 0x1000_0000;

#[macro_use]
use crate::drivers::chardev::{CharDevice, UART};
use crate::drivers::plic::{IntrTargetPriority, PLIC};
use crate::drivers::BLOCK_DEVICE;
use crate::log;

pub fn device_init() {
    use riscv::register::sie;
    let mut plic = unsafe { PLIC::new(VIRT_PLIC) };
    let hart_id: usize = 0;
    let supervisor = IntrTargetPriority::Supervisor;
    let machine = IntrTargetPriority::Machine;
    plic.set_threshold(hart_id, supervisor, 0);
    plic.set_threshold(hart_id, machine, 1); // machine模式 阈值为1, 优先级<=1的中断源不会触发中断
                                             //irq num:5 keyboard, 6 mouse , 8 block , 10 uart
    for intr_src_id in [5usize, 6, 8, 10] {
        plic.enable(hart_id, supervisor, intr_src_id); //这些中断源打开中断
        plic.set_priority(intr_src_id, 1); //中断源设置优先级
    }
    unsafe {
        sie::set_sext(); // riscv CPU 打开外部中断
    }
}

pub fn irq_handler() {
    let mut plic = unsafe { PLIC::new(VIRT_PLIC) };
    let intr_src_id = plic.claim(0, IntrTargetPriority::Supervisor); //第0个CPU (反正我们单CPU) 操作系统的特权:Supervisor
    match intr_src_id {
        // 5 => KEYBOARD_DEVICE.handle_irq(),
        // 6 => MOUSE_DEVICE.handle_irq(),
        8 => {
            // log!( "\x1b[35m[qemu: irq_handler] trap_from_kernel call block device handle_irq [{}]  \x1b[0m", intr_src_id);
            BLOCK_DEVICE.handle_irq()
        }
        10 => UART.handle_irq(),
        _ => panic!(
            "unsupported IRQ {}  现在只接受5/6/8/10 对应就是鼠标|键盘|磁盘|串口",
            intr_src_id
        ),
    }
    plic.complete(0, IntrTargetPriority::Supervisor, intr_src_id);
}
