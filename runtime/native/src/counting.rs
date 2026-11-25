use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};

static COUNTING: AtomicBool = AtomicBool::new(false);
static LIVE: AtomicIsize = AtomicIsize::new(0);

pub struct Counting;

#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn counted(delta: isize) {
    if COUNTING.load(Ordering::Relaxed) {
        LIVE.fetch_add(delta, Ordering::Relaxed);
    }
}

// SAFETY: every method forwards to `System` with the same arguments and only counts around it.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller upholds `GlobalAlloc::alloc`'s contract, which `System` shares.
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            counted(1);
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller upholds `GlobalAlloc::alloc_zeroed`'s contract.
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            counted(1);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        counted(-1);
        // SAFETY: the caller upholds `GlobalAlloc::dealloc`'s contract.
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the caller upholds `GlobalAlloc::realloc`'s contract.
        unsafe { System.realloc(pointer, layout, new_size) }
    }
}

pub fn start() {
    LIVE.store(0, Ordering::Relaxed);
    COUNTING.store(true, Ordering::Relaxed);
}

pub fn stop() -> Option<isize> {
    COUNTING
        .swap(false, Ordering::Relaxed)
        .then(|| LIVE.load(Ordering::Relaxed))
}
