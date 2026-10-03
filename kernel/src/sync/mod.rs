//! Synchronization primitives for FLX Kernel.
//!
//! Provides bare-metal synchronization mechanisms designed for `no_std` environments
//! where OS threads or scheduler primitives are not yet present.

pub mod spinlock;

pub use spinlock::{Spinlock, SpinlockGuard};
