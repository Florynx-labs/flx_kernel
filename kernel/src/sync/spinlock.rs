//! Spinlock mutual exclusion primitive.
//!
//! Designed for single-core or multicore low-level mutual exclusion
//! without relying on heap allocation or OS thread scheduling.

use core::cell::UnsafeCell;
use core::hint::spin_loop;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, Ordering};

/// A simple, robust spinlock.
///
/// Wraps protected data in an `UnsafeCell` and uses an atomic boolean flag
/// with acquire-release semantics to ensure mutually exclusive access.
pub struct Spinlock<T: ?Sized> {
    locked: AtomicBool,
    data: UnsafeCell<T>,
}

// SAFETY: A spinlock can be transferred across threads if the underlying type is Send.
unsafe impl<T: ?Sized + Send> Send for Spinlock<T> {}
// SAFETY: Spinlock provides synchronized mutable access, so it is Sync if T is Send.
unsafe impl<T: ?Sized + Send> Sync for Spinlock<T> {}

impl<T> Spinlock<T> {
    /// Creates a new unlocked `Spinlock` containing the given data.
    pub const fn new(data: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            data: UnsafeCell::new(data),
        }
    }

    /// Consumes the spinlock and returns the underlying data.
    pub fn into_inner(self) -> T {
        self.data.into_inner()
    }
}

impl<T: ?Sized> Spinlock<T> {
    /// Acquires the lock, spinning until it is available.
    ///
    /// Returns an RAII guard that unlocks when dropped.
    pub fn lock(&self) -> SpinlockGuard<'_, T> {
        // Spin until we successfully swap false -> true with Acquire ordering.
        while self
            .locked
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            // Give CPU hyperthreading hint to reduce power/pipeline contention
            while self.is_locked() {
                spin_loop();
            }
        }

        SpinlockGuard { lock: self }
    }

    /// Attempts to acquire the lock without blocking.
    ///
    /// Returns `Some(guard)` if acquired, or `None` if already locked.
    pub fn try_lock(&self) -> Option<SpinlockGuard<'_, T>> {
        if self
            .locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            Some(SpinlockGuard { lock: self })
        } else {
            None
        }
    }

    /// Returns `true` if the lock is currently held.
    #[inline]
    pub fn is_locked(&self) -> bool {
        self.locked.load(Ordering::Relaxed)
    }

    /// Forcefully releases the lock.
    ///
    /// # Safety
    /// This is unsafe because it can violate mutual exclusion if called while
    /// another context is holding the lock. Intended strictly for emergency/panic resets.
    pub unsafe fn force_unlock(&self) {
        self.locked.store(false, Ordering::Release);
    }
}

/// An RAII implementation of a "scoped lock" of a spinlock.
///
/// When this structure is dropped (falls out of scope), the lock will be unlocked.
pub struct SpinlockGuard<'a, T: ?Sized> {
    lock: &'a Spinlock<T>,
}

impl<'a, T: ?Sized> Deref for SpinlockGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        // SAFETY: We hold the spinlock, so unique access to `data` is guaranteed.
        unsafe { &*self.lock.data.get() }
    }
}

impl<'a, T: ?Sized> DerefMut for SpinlockGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: We hold the spinlock, so unique mutable access to `data` is guaranteed.
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<'a, T: ?Sized> Drop for SpinlockGuard<'a, T> {
    fn drop(&mut self) {
        // Release the lock with Release ordering so all memory operations
        // inside the critical section become visible before the unlock.
        self.lock.locked.store(false, Ordering::Release);
    }
}
