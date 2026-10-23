//! x86_64 CPU low-level controls and hardware identification.

use core::arch::x86_64::__cpuid;

/// CPU vendor string (12 ASCII characters).
#[derive(Debug, Clone, Copy)]
pub struct CpuVendor {
    bytes: [u8; 12],
}

impl CpuVendor {
    /// Returns the vendor string as a UTF-8 string slice.
    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes).unwrap_or("Unknown")
    }
}

/// Identifies the CPU vendor using CPUID leaf 0.
pub fn get_cpu_vendor() -> CpuVendor {
    // Leaf 0: Highest Function Parameter and Manufacturer ID.
    // SAFETY: CPUID instruction is always available on x86_64 and has no side effects.
    // The `unsafe` block is required by the std API on stable Rust but contains no UB.
    #[allow(unused_unsafe)]
    let result = unsafe { __cpuid(0) };

    let mut bytes = [0u8; 12];
    // EBX contains first 4 bytes
    bytes[0..4].copy_from_slice(&result.ebx.to_le_bytes());
    // EDX contains middle 4 bytes
    bytes[4..8].copy_from_slice(&result.edx.to_le_bytes());
    // ECX contains last 4 bytes
    bytes[8..12].copy_from_slice(&result.ecx.to_le_bytes());

    CpuVendor { bytes }
}

/// Disables maskable CPU interrupts on the current core (`cli`).
#[inline]
pub fn disable_interrupts() {
    unsafe {
        core::arch::asm!("cli", options(nomem, nostack));
    }
}

/// Enables maskable CPU interrupts on the current core (`sti`).
#[inline]
pub fn enable_interrupts() {
    unsafe {
        core::arch::asm!("sti", options(nomem, nostack));
    }
}

/// Returns `true` if maskable interrupts are currently enabled (RFLAGS.IF).
#[inline]
pub fn interrupts_enabled() -> bool {
    let flags: u64;
    // SAFETY: `pushfq; pop` only reads RFLAGS. It uses the stack, so no `nostack` option.
    unsafe {
        core::arch::asm!("pushfq", "pop {}", out(reg) flags, options(nomem, preserves_flags));
    }
    flags & (1 << 9) != 0
}

/// Reads CR2 (faulting virtual address after a page fault).
#[inline]
pub fn read_cr2() -> u64 {
    let value: u64;
    // SAFETY: Reading CR2 has no side effects.
    unsafe {
        core::arch::asm!("mov {}, cr2", out(reg) value, options(nomem, nostack, preserves_flags));
    }
    value
}

/// Puts the CPU into a low-power halt state until the next interrupt arrives (`hlt`).
#[inline]
pub fn halt() {
    unsafe {
        core::arch::asm!("hlt", options(nomem, nostack));
    }
}

/// Halts the CPU indefinitely in a safe low-power loop.
pub fn halt_loop() -> ! {
    loop {
        halt();
    }
}
