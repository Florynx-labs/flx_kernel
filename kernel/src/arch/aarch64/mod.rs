//! AArch64 (ARM64) low-level CPU primitives.
//!
//! This module is deliberately independent of x86-specific descriptors and port I/O.
//! It is the first step toward a multi-architecture kernel; the current UEFI boot path and
//! shared drivers are still x86_64-only.

pub const ARCH_NAME: &str = "aarch64";
pub const PAGE_SIZE: usize = 4096;

/// Waits for an interrupt/event forever.
#[cfg(target_arch = "aarch64")]
pub fn halt() -> ! {
    loop {
        // SAFETY: WFE is available to the kernel at EL1 and has no memory operands.
        unsafe { core::arch::asm!("wfe", options(nomem, nostack)); }
    }
}

/// Masks IRQ, FIQ, SError, and debug exceptions at the current exception level.
#[cfg(target_arch = "aarch64")]
pub fn disable_interrupts() {
    // SAFETY: DAIFSet is available to privileged kernel code.
    unsafe { core::arch::asm!("msr daifset, #0xf", options(nomem, nostack, preserves_flags)); }
}

/// Unmasks IRQ, FIQ, SError, and debug exceptions at the current exception level.
#[cfg(target_arch = "aarch64")]
pub fn enable_interrupts() {
    // SAFETY: DAIFClr is available to privileged kernel code.
    unsafe { core::arch::asm!("msr daifclr, #0xf", options(nomem, nostack, preserves_flags)); }
}

/// Returns the current exception level (EL0–EL3).
#[cfg(target_arch = "aarch64")]
pub fn current_exception_level() -> u8 {
    let value: u64;
    // SAFETY: CurrentEL is readable from privileged code.
    unsafe { core::arch::asm!("mrs {value}, CurrentEL", value = out(reg) value, options(nomem, nostack, preserves_flags)); }
    ((value >> 2) & 0b11) as u8
}

/// Returns the affinity identifier for this logical CPU.
#[cfg(target_arch = "aarch64")]
pub fn mpidr_el1() -> u64 {
    let value: u64;
    // SAFETY: MPIDR_EL1 is readable from EL1.
    unsafe { core::arch::asm!("mrs {value}, MPIDR_EL1", value = out(reg) value, options(nomem, nostack, preserves_flags)); }
    value
}

/// Installs the physical root for the lower VA range (TTBR0_EL1).
///
/// # Safety
/// The root must be a 4 KiB-aligned physical address of a valid translation table, and the
/// active TCR/MAIR configuration must describe that table. The caller must handle TLBs.
#[cfg(target_arch = "aarch64")]
pub unsafe fn write_ttbr0_el1(root_phys: u64) {
    debug_assert_eq!(root_phys & (PAGE_SIZE as u64 - 1), 0);
    // SAFETY: the caller guarantees a valid translation-table root.
    unsafe {
        core::arch::asm!(
            "dsb ishst",
            "msr TTBR0_EL1, {root}",
            "isb",
            root = in(reg) root_phys,
            options(nostack, preserves_flags)
        );
    }
}

/// Invalidates all EL1 stage-1 TLB entries on the current CPU.
#[cfg(target_arch = "aarch64")]
pub unsafe fn invalidate_tlb_all_el1() {
    // SAFETY: caller must coordinate other CPUs and address spaces around TLB maintenance.
    unsafe {
        core::arch::asm!(
            "dsb ishst",
            "tlbi vmalle1",
            "dsb ish",
            "isb",
            options(nostack, preserves_flags)
        );
    }
}
