//! Compile-only cross-check for the experimental RISC-V 64 architecture primitives.

#![no_std]

#[path = "../../../kernel/src/arch/riscv64/mod.rs"]
pub mod riscv64;
