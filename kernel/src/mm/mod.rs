//! Memory management subsystem.
//!
//! v0.2.0: physical frame allocator.
//! v0.3.0: active page-table primitives and a fixed-capacity virtual range allocator.
//! The virtual range allocator reserves addresses only; heap and process address spaces
//! will be layered on top after a kernel virtual address arena is defined safely.

pub mod frame;
pub mod paging;
pub mod virtual_range;
