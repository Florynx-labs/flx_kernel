//! Memory management subsystem.
//!
//! v0.2.0: physical frame allocator.
//! v0.3.0 foundation: active x86_64 page-table inspection and 4 KiB map/unmap primitives.
//! Heap allocation and isolated address spaces remain future work.

pub mod frame;
pub mod paging;
