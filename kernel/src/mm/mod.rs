//! Memory management subsystem.
//!
//! v0.2.0: physical frame allocator.
//! v0.3.0: active page-table primitives and a fixed-capacity virtual range allocator.
//! ELF support currently validates bounded static x86_64 executable images; mapping and
//! user-mode startup are separate follow-up milestones.

pub mod elf;
pub mod frame;
pub mod heap;
pub mod paging;
pub mod virtual_range;
