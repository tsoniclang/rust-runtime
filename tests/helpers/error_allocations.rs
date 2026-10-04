use core::cell::Cell;
use std::alloc::{GlobalAlloc, Layout, System};

struct CountingAllocator;

thread_local! {
    pub static ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) };
    pub static ALLOCATION_BYTES: Cell<Option<usize>> = const { Cell::new(None) };
}

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATION_BYTES.with(|bytes| {
            if let Some(value) = bytes.get() {
                bytes.set(Some(value + layout.size()));
            }
        });
        ALLOCATIONS.with(|count| {
            if let Some(value) = count.get() {
                count.set(Some(value + 1));
            }
        });
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        ALLOCATIONS.with(|count| {
            if let Some(value) = count.get() {
                count.set(Some(value + 1));
            }
        });
        unsafe { System.realloc(pointer, layout, size) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;
