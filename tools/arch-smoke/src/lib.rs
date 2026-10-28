#![no_std]

// Compile the exact kernel architecture module in isolation so ARM64 assembly can be
// cross-checked without pretending the x86_64-only bootloader/kernel path is portable yet.
#[path = "../../../kernel/src/arch/aarch64/mod.rs"]
pub mod aarch64;
