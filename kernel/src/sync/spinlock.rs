//! Spinlock mutual exclusion primitive.
//!
//! Designed for single-core or multicore low-level mutual exclusion
//! without relying on heap allocation or OS thread scheduling.

use core::cell::UnsafeCell;
use core::hint::spin_loop;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, Ordering};

use crate::arch::x86_64::cpu;

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
    ///
    /// Interrupts are disabled on the local core while the lock is held and restored to
    /// their previous state on drop. This prevents an interrupt handler from deadlocking
    /// on a lock already held by the code it interrupted (e.g. logging from an IRQ).
    pub fn lock(&self) -> SpinlockGuard<'_, T> {
        let irq_was_enabled = cpu::interrupts_enabled();
        cpu::disable_interrupts();

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

        SpinlockGuard { lock: self, irq_was_enabled }
    }

    /// Attempts to acquire the lock without blocking.
    ///
    /// Returns `Some(guard)` if acquired, or `None` if already locked.
    pub fn try_lock(&self) -> Option<SpinlockGuard<'_, T>> {
        let irq_was_enabled = cpu::interrupts_enabled();
        cpu::disable_interrupts();

        if self
            .locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            Some(SpinlockGuard { lock: self, irq_was_enabled })
        } else {
            if irq_was_enabled {
                cpu::enable_interrupts();
            }
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
    irq_was_enabled: bool,
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
        if self.irq_was_enabled {
            cpu::enable_interrupts();
        }
    }
}
