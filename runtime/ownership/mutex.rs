use std::ops::{Deref, DerefMut};
use std::sync::{Condvar, Mutex as StateLock};

pub struct Mutex<T> {
    available: StateLock<Option<T>>,
    unlocked: Condvar,
}

/// The empty slot denotes acquisition. The guard owns the payload until it
/// publishes it back into the slot, without holding a thread-affine OS lock.
pub struct LockGuard<'a, T> {
    source: &'a Mutex<T>,
    payload: Option<T>,
}

pub fn new<T>(value: T) -> Mutex<T> {
    Mutex {
        available: StateLock::new(Some(value)),
        unlocked: Condvar::new(),
    }
}

pub fn acquire<T>(source: &Mutex<T>) -> LockGuard<'_, T> {
    let mut available = source
        .unlocked
        .wait_while(source.available.lock().unwrap(), |slot| slot.is_none())
        .unwrap();
    LockGuard {
        source,
        payload: available.take(),
    }
}

pub fn release<T>(guard: LockGuard<'_, T>) {
    drop(guard);
}

impl<T> Deref for LockGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.payload.as_ref().unwrap()
    }
}

impl<T> DerefMut for LockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.payload.as_mut().unwrap()
    }
}

impl<T> Drop for LockGuard<'_, T> {
    fn drop(&mut self) {
        *self.source.available.lock().unwrap() = self.payload.take();
        self.source.unlocked.notify_one();
    }
}
