//! Host-side test harness for pure memory-management algorithms.
//! Kernel source modules are included directly so tests exercise production code.

pub mod sync {
    use std::sync::{Mutex, MutexGuard};

    /// Host-only stand-in for the kernel spinlock; privileged IRQ operations are not tested here.
    pub struct Spinlock<T>(Mutex<T>);

    impl<T> Spinlock<T> {
        pub const fn new(value: T) -> Self {
            Self(Mutex::new(value))
        }

        pub fn lock(&self) -> MutexGuard<'_, T> {
            self.0.lock().expect("host test lock poisoned")
        }
    }
}

pub mod mm;

#[path = "../../../kernel/src/mm/heap.rs"]
pub mod heap;

#[path = "../../../kernel/src/mm/virtual_range.rs"]
pub mod virtual_range;

pub use mm::paging;

// Exercise the production scheduler policy in the host-side unit-test harness.
#[path = "../../../kernel/src/task/scheduler.rs"]
pub mod scheduler;
