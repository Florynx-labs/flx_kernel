#![no_std]

// Compile the exact kernel architecture modules in isolation. The aliases reproduce the
// crate::arch paths used by the real kernel while keeping #[path] rooted at this file.
#[path = "../../../kernel/src/arch/interface.rs"]
pub mod interface;

#[path = "../../../kernel/src/arch/aarch64/mod.rs"]
pub mod aarch64;

pub mod arch {
    pub use crate::aarch64;
    pub use crate::interface;
}
