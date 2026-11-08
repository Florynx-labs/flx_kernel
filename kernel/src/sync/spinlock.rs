//! Spinlock mutual exclusion primitive.
//!
//! Low-level mutual exclusion without heap allocation or OS thread scheduling.
//! Architecture-specific interrupt instructions are reached only through the arch facade.

use core::cell::UnsafeCell;
use core::hint::spin_loop;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, Ordering};
use crate::arch;

pub struct Spinlock<T: ?Sized> {
    locked: AtomicBool,
    data: UnsafeCell<T>,
}
unsafe impl<T: ?Sized + Send> Send for Spinlock<T> {}
unsafe impl<T: ?Sized + Send> Sync for Spinlock<T> {}

impl<T> Spinlock<T> {
    pub const fn new(data: T) -> Self {
        Self { locked: AtomicBool::new(false), data: UnsafeCell::new(data) }
    }
    pub fn into_inner(self) -> T { self.data.into_inner() }
}

impl<T: ?Sized> Spinlock<T> {
    /// Disables local maskable interrupts while held and restores their prior state on drop.
    /// NMI/FIQ-style non-maskable contexts require separate locking rules.
    pub fn lock(&self) -> SpinlockGuard<'_, T> {
        let irq_was_enabled = arch::local_interrupts_enabled();
        arch::disable_local_interrupts();
        while self.locked.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            while self.is_locked() { spin_loop(); }
        }
        SpinlockGuard { lock: self, irq_was_enabled }
    }

    pub fn try_lock(&self) -> Option<SpinlockGuard<'_, T>> {
        let irq_was_enabled = arch::local_interrupts_enabled();
        arch::disable_local_interrupts();
        if self.locked.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_ok() {
            Some(SpinlockGuard { lock: self, irq_was_enabled })
        } else {
            if irq_was_enabled { arch::enable_local_interrupts(); }
            None
        }
    }

    #[inline]
    pub fn is_locked(&self) -> bool { self.locked.load(Ordering::Relaxed) }

    /// # Safety
    /// Can violate mutual exclusion if another context currently holds the lock.
    pub unsafe fn force_unlock(&self) { self.locked.store(false, Ordering::Release); }
}

pub struct SpinlockGuard<'a, T: ?Sized> {
    lock: &'a Spinlock<T>,
    irq_was_enabled: bool,
}
impl<T: ?Sized> Deref for SpinlockGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        // SAFETY: The guard owns the lock, so unique access is guaranteed.
        unsafe { &*self.lock.data.get() }
    }
}
impl<T: ?Sized> DerefMut for SpinlockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: The guard owns the lock, so unique mutable access is guaranteed.
        unsafe { &mut *self.lock.data.get() }
    }
}
impl<T: ?Sized> Drop for SpinlockGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.locked.store(false, Ordering::Release);
        if self.irq_was_enabled { arch::enable_local_interrupts(); }
    }
}
