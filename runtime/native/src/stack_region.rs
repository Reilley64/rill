use std::sync::atomic::{AtomicUsize, Ordering};

static LOW: AtomicUsize = AtomicUsize::new(0);
static HIGH: AtomicUsize = AtomicUsize::new(0);

pub fn publish(low: usize, high: usize) {
    LOW.store(low, Ordering::Relaxed);
    HIGH.store(high, Ordering::Relaxed);
}

/// Writes the bounds of the entry stack's usable bytes to `low` and `high`: the lowest usable
/// address and one past the highest. Both are 0 outside the entry stack's lifetime, that is
/// before `main` switches to it and after it switches back. The guard page lies just below `low`.
/// Test support, in the checking archive only.
///
/// # Safety
///
/// `low` and `high` are each valid for one write.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rill_rt_entry_stack_region(low: *mut usize, high: *mut usize) {
    // SAFETY: the caller passes writable `low` and `high`.
    unsafe {
        *low = LOW.load(Ordering::Relaxed);
        *high = HIGH.load(Ordering::Relaxed);
    }
}
