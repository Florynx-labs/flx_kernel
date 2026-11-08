//! AArch64 (ARM64) low-level CPU primitives.
//!
//! This module is deliberately independent of x86-specific descriptors and port I/O.
//! It is the first step toward a multi-architecture kernel; the current UEFI boot path and
//! shared drivers are still x86_64-only.

pub const ARCH_NAME: &str = "aarch64";
pub const PAGE_SIZE: usize = 4096;

#[cfg(target_arch = "aarch64")]
pub mod exceptions;

/// Waits for an interrupt/event forever.
#[cfg(target_arch = "aarch64")]
pub fn halt() -> ! {
    loop {
        // SAFETY: WFE is available to the kernel at EL1 and has no memory operands.
        unsafe { core::arch::asm!("wfe", options(nomem, nostack)); }
    }
}

/// Masks IRQ exceptions only, preserving the caller's FIQ, SError, and debug mask state.
#[cfg(target_arch = "aarch64")]
pub fn disable_interrupts() {
    // SAFETY: DAIFSet is available to privileged kernel code.
    unsafe { core::arch::asm!("msr daifset, #2", options(nomem, nostack, preserves_flags)); }
}

/// Unmasks IRQ exceptions only; other DAIF mask bits are left unchanged.
#[cfg(target_arch = "aarch64")]
pub fn enable_interrupts() {
    // SAFETY: DAIFClr is available to privileged kernel code.
    unsafe { core::arch::asm!("msr daifclr, #2", options(nomem, nostack, preserves_flags)); }
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


/// Native AArch64 implementation of the architecture-neutral CPU contract.
#[cfg(target_arch = "aarch64")]
pub struct AArch64;

#[cfg(target_arch = "aarch64")]

impl crate::arch::interface::CpuArchitecture for AArch64 {
    const NAME: &'static str = "aarch64";

    fn disable_interrupts() { disable_interrupts(); }
    fn enable_interrupts() { enable_interrupts(); }

    fn interrupts_enabled() -> bool {
        let daif: u64;
        // SAFETY: DAIF is readable from privileged EL1 code.
        unsafe { core::arch::asm!("mrs {value}, daif", value = out(reg) daif, options(nomem, nostack, preserves_flags)); }
        daif & (1 << 7) == 0
    }

    fn logical_cpu_id() -> u64 {
        // Keep the affinity fields (Aff3:Aff0), excluding MPIDR status/reserved bits.
        mpidr_el1() & 0x0000_00ff_00ff_ffff
    }

    fn halt_forever() -> ! { halt() }

    fn nx_enabled() -> bool {
        // This x86 four-level paging backend's NX bit is not an AArch64 UXN/PXN descriptor.
        // AArch64 must use its own translation-table backend before this capability is exposed.
        false
    }

    fn current_address_space_root() -> u64 {
        let root: u64;
        // SAFETY: TTBR0_EL1 is readable by privileged EL1 code.
        unsafe { core::arch::asm!("mrs {root}, TTBR0_EL1", root = out(reg) root, options(nomem, nostack, preserves_flags)); }
        root & 0x0000_ffff_ffff_f000
    }

    unsafe fn invalidate_tlb_page(address: u64) {
        let va = address >> 12;
        // SAFETY: Caller coordinates mapping mutation and remote shootdowns.
        unsafe {
            core::arch::asm!(
                "dsb ishst",
                "tlbi vaae1is, {va}",
                "dsb ish",
                "isb",
                va = in(reg) va,
                options(nostack, preserves_flags)
            );
        }
    }

    unsafe fn activate_address_space(root_phys: u64) {
        // SAFETY: forwarded from the trait caller; this also performs the required TLB flush.
        unsafe {
            write_ttbr0_el1(root_phys);
            invalidate_tlb_all_el1();
        }
    }

    unsafe fn invalidate_tlb_all() {
        // SAFETY: forwarded from the trait caller.
        unsafe { invalidate_tlb_all_el1(); }
    }
}
