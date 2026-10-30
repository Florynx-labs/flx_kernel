#![no_std]

// Compile the exact kernel architecture modules in isolation. This checks the AArch64
// privileged instructions and the shared interface without claiming a complete ARM64 boot.
pub mod arch {
    #[path = "../../../kernel/src/arch/interface.rs"]
    pub mod interface;

    #[path = "../../../kernel/src/arch/aarch64/mod.rs"]
    pub mod aarch64;
}
