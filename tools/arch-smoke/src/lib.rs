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

    #[inline]
    pub fn local_interrupts_enabled() -> bool {
        <aarch64::AArch64 as interface::CpuArchitecture>::interrupts_enabled()
    }
    #[inline]
    pub fn disable_local_interrupts() {
        <aarch64::AArch64 as interface::CpuArchitecture>::disable_interrupts()
    }
    #[inline]
    pub fn enable_local_interrupts() {
        <aarch64::AArch64 as interface::CpuArchitecture>::enable_interrupts()
    }
}

// Compile the real shared spinlock against the AArch64 implementation. This catches
// accidental x86-only dependencies in common synchronization code.
#[path = "../../../kernel/src/sync/spinlock.rs"]
pub mod spinlock;
