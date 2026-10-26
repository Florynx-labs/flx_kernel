//! Host harness module exposing production memory-management modules.
#[path = "../../../../kernel/src/mm/elf.rs"]
pub mod elf;

#[path = "../../../../kernel/src/mm/elf_loader.rs"]
pub mod elf_loader;

#[path = "../../../../kernel/src/mm/frame.rs"]
pub mod frame;

#[path = "../../../../kernel/src/mm/paging.rs"]
pub mod paging;
