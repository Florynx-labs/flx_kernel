//! Memory management subsystem.
//!
//! v0.2.0: physical frame allocator.
//! v0.3.0: active page-table primitives and a fixed-capacity virtual range allocator.
//! ELF support validates and maps bounded static x86_64 executable images into isolated
//! address spaces; user-mode startup and stack construction remain follow-up milestones.

pub mod elf;
pub mod elf_loader;
pub mod frame;
pub mod heap;
pub mod paging;
pub mod virtual_range;
