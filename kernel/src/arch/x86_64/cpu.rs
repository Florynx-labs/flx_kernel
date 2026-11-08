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


/// Native x86_64 implementation of the architecture-neutral CPU contract.
#[cfg(target_arch = "x86_64")]
pub struct X86_64;

#[cfg(target_arch = "x86_64")]

impl crate::arch::interface::CpuArchitecture for X86_64 {
    const NAME: &'static str = "x86_64";

    fn disable_interrupts() { disable_interrupts(); }
    fn enable_interrupts() { enable_interrupts(); }
    fn interrupts_enabled() -> bool { interrupts_enabled() }

    fn logical_cpu_id() -> u64 {
        // CPUID leaf 1 EBX[31:24] reports the initial APIC ID on supported x86 CPUs.
        // This is sufficient for the current single-package bring-up; topology discovery
        // and x2APIC IDs will be added with the SMP/APIC milestone.
        let result = unsafe { __cpuid(1) };
        ((result.ebx >> 24) & 0xff) as u64
    }

    fn halt_forever() -> ! { halt_loop() }

    fn nx_enabled() -> bool {
        const CPUID_NX: u32 = 1 << 20;
        // SAFETY: CPUID is available on x86_64 and these leaves are side-effect free.
        let max = unsafe { __cpuid(0x8000_0000) }.eax;
        if max < 0x8000_0001 { return false; }
        let features = unsafe { __cpuid(0x8000_0001) };
        if features.edx & CPUID_NX == 0 { return false; }
        let low: u32;
        let high: u32;
        // SAFETY: EFER is readable at CPL0; kernel runs privileged.
        unsafe {
            core::arch::asm!("rdmsr", in("ecx") 0xC000_0080u32,
                out("eax") low, out("edx") high, options(nomem, nostack, preserves_flags));
        }
        (((high as u64) << 32) | low as u64) & (1 << 11) != 0
    }

    fn current_address_space_root() -> u64 {
        let root: u64;
        // SAFETY: Reading CR3 is a privileged, side-effect-free register read.
        unsafe { core::arch::asm!("mov {}, cr3", out(reg) root, options(nomem, nostack, preserves_flags)); }
        root & 0x000f_ffff_ffff_f000
    }

    unsafe fn invalidate_tlb_page(address: u64) {
        // SAFETY: The caller guarantees the active translation context and shootdown protocol.
        unsafe { core::arch::asm!("invlpg [{}]", in(reg) address, options(nostack, preserves_flags)); }
    }

    unsafe fn activate_address_space(root_phys: u64) {
        assert_eq!(root_phys & 0xfff, 0, "CR3 root must be 4 KiB aligned");
        // SAFETY: upheld by the trait caller's contract.
        unsafe { core::arch::asm!("mov cr3, {}", in(reg) root_phys, options(nostack, preserves_flags)); }
    }

    unsafe fn invalidate_tlb_all() {
        // Reloading CR3 flushes non-global translations when PCID is not enabled.
        // Global mappings are intentionally retained; use INVPCID when PCID support lands.
        let root: u64;
        unsafe {
            core::arch::asm!("mov {}, cr3", out(reg) root, options(nomem, nostack, preserves_flags));
            core::arch::asm!("mov cr3, {}", in(reg) root, options(nostack, preserves_flags));
        }
    }
}
