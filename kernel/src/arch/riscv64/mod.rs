//! RISC-V 64 supervisor-mode primitives for the experimental FLX bring-up path.
//!
//! This module is deliberately not wired into the production kernel entry or CpuArchitecture
//! trait yet. It contains real S-mode CSR operations and an SBI call boundary; boot handoff,
//! trap dispatch, PLIC, MMU, and scheduler integration remain separate milestones.

use core::arch::asm;

/// Stable architecture identifier used by diagnostics and support reports.
pub const ARCH_NAME: &str = "riscv64";
/// SBI TIME extension identifier (SBI v0.2+).
pub const SBI_EXT_TIME: usize = 0x5449_4d45;
/// SBI TIME function ID for setting the next absolute timer event.
pub const SBI_TIME_SET_TIMER: usize = 0;
/// Supervisor interrupt-enable bit in `sstatus`.
const SSTATUS_SIE: usize = 1 << 1;
/// Direct-mode `stvec` requires a four-byte aligned base address.
pub const TRAP_VECTOR_ALIGNMENT: usize = 4;

/// SBI return registers: a0 carries the error and a1 carries the extension value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct SbiRet {
    pub error: isize,
    pub value: usize,
}

impl SbiRet {
    /// Returns true only when SBI reports success (error code zero).
    pub const fn is_success(self) -> bool {
        self.error == 0
    }
}

/// Errors detected before publishing the supervisor trap-vector base.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrapVectorError {
    Misaligned { address: usize },
}

/// Returns whether supervisor-level maskable interrupts are enabled.
#[inline]
pub fn interrupts_enabled() -> bool {
    let status: usize;
    // SAFETY: sstatus is readable by supervisor-mode kernel code.
    unsafe {
        asm!(
            "csrr {status}, sstatus",
            status = out(reg) status,
            options(nomem, nostack)
        );
    }
    status & SSTATUS_SIE != 0
}

/// Masks supervisor-level maskable interrupts on the current hart.
#[inline]
pub fn disable_interrupts() {
    // SAFETY: csrc sstatus clears only the SIE bit supplied in the mask.
    unsafe {
        asm!(
            "csrc sstatus, {mask}",
            mask = in(reg) SSTATUS_SIE,
            options(nomem, nostack)
        );
    }
}

/// Unmasks supervisor-level maskable interrupts on the current hart.
///
/// The caller must install a valid trap vector and initialize the interrupt source first.
#[inline]
pub fn enable_interrupts() {
    // SAFETY: csrs sstatus sets only the SIE bit supplied in the mask.
    unsafe {
        asm!(
            "csrs sstatus, {mask}",
            mask = in(reg) SSTATUS_SIE,
            options(nomem, nostack)
        );
    }
}

/// Waits until an interrupt or implementation-defined event allows execution to resume.
///
/// This does not mask interrupts and is not a scheduler yield.
#[inline]
pub fn wait_for_interrupt() {
    // SAFETY: WFI is a standard privileged RISC-V instruction.
    unsafe { asm!("wfi", options(nomem, nostack)); }
}

/// Installs a direct-mode supervisor trap-vector base.
///
/// # Safety
/// `entry` must point to a mapped, executable supervisor trap entry that preserves the
/// interrupted context and follows the kernel's eventual trap-frame ABI. The caller must
/// arrange a valid stack and must keep interrupts masked until the handler is ready.
pub unsafe fn install_trap_vector(entry: usize) -> Result<(), TrapVectorError> {
    if entry % TRAP_VECTOR_ALIGNMENT != 0 {
        return Err(TrapVectorError::Misaligned { address: entry });
    }

    // stvec.MODE=0 selects direct mode; the low two bits are therefore zero.
    // SAFETY: the caller guarantees that entry is a valid executable trap handler.
    unsafe {
        asm!(
            "csrw stvec, {entry}",
            entry = in(reg) entry,
            options(nomem, nostack)
        );
    }
    Ok(())
}

/// Performs an SBI v0.2+ ecall using the standard a0–a7 register convention.
///
/// # Safety
/// Must be called in supervisor mode with a compatible SBI implementation available.
/// The caller must pass arguments valid for the selected extension/function pair.
pub unsafe fn sbi_call(
    extension_id: usize,
    function_id: usize,
    args: [usize; 6],
) -> SbiRet {
    let error: isize;
    let value: usize;

    // SAFETY: the caller guarantees S-mode and a valid SBI extension/function contract.
    unsafe {
        asm!(
            "ecall",
            inlateout("a0") args[0] => error,
            inlateout("a1") args[1] => value,
            in("a2") args[2],
            in("a3") args[3],
            in("a4") args[4],
            in("a5") args[5],
            in("a6") function_id,
            in("a7") extension_id,
            options(nostack)
        );
    }

    SbiRet { error, value }
}

/// Programs the next absolute supervisor timer event through the SBI TIME extension.
///
/// # Safety
/// Requires supervisor mode and an SBI implementation supporting extension TIME (SBI v0.2+).
/// The caller must install a valid trap handler before unmasking supervisor interrupts.
pub unsafe fn set_timer(deadline: u64) -> Result<(), SbiRet> {
    // SBI RV64 passes the 64-bit absolute deadline in a0.
    let result = unsafe {
        sbi_call(
            SBI_EXT_TIME,
            SBI_TIME_SET_TIMER,
            [deadline as usize, 0, 0, 0, 0, 0],
        )
    };

    if result.is_success() {
        Ok(())
    } else {
        Err(result)
    }
}
