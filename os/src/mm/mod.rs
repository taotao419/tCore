//! Memory management implementation
//!
//! SV39 page-based virtual-memory architecture for RV64 systems, and
//! everything about memory management, like frame allocator, page table,
//! map area and memory set, is implemented here.
//!
//! Every task or process has a memory_set to control its virtual memory.

mod address;
mod frame_allocator;
mod heap_allocator;
mod memory_set;
mod page_table;

pub use address::{ VPNRange,PPNRange};
pub use address::{PhysAddr, PhysPageNum,StepByOne, VirtAddr, VirtPageNum};
pub use frame_allocator::{frame_alloc, frame_dealloc,FrameTracker};
pub use memory_set::remap_test;
pub use memory_set::{kernel_token,MapArea,MapPermission,MapType,SectionType, MemorySet, KERNEL_SPACE};
use page_table::PTEFlags;
pub use page_table::{translated_byte_buffer,translated_ref, translated_refmut, translated_str,PageTable, PageTableEntry,UserBuffer};

/// initiate heap allocator, frame allocator and kernel space
pub fn init() {
    heap_allocator::init_heap();
    frame_allocator::init_frame_allocator();
    KERNEL_SPACE.exclusive_access().activate();
}
