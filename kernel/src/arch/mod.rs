//! Architecture-specific code for FLX Kernel.
//! Shared kernel services must depend on explicit architecture interfaces, not on
//! x86_64 register layouts or port I/O.

pub mod x86_64;

/// Early bring-up primitives for AArch64. The full kernel boot path is not wired yet.
pub mod aarch64;
