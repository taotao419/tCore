mod heap_allocator;

/// initiate heap allocator, frame allocator and kernel space
pub fn init() {
    heap_allocator::init_heap();
}

pub fn test_heap(){
    heap_allocator::heap_test();
}