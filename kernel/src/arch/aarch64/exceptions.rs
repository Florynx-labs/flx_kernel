//! AArch64 EL1 exception vector table and controlled exception diagnostics.
//!
//! The installer masks all exception classes before publishing VBAR_EL1. IRQs remain
//! masked until the caller explicitly enables them after its interrupt controller is ready.

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

pub const VECTOR_TABLE_ALIGNMENT: usize = 2048;
const CONTROLLED_BRK_IMMEDIATE: u64 = 0x0f00;
const ESR_EC_BRK64: u64 = 0x3c;

static CONTROLLED_EXCEPTION_SEEN: AtomicBool = AtomicBool::new(false);
static LAST_ESR: AtomicU64 = AtomicU64::new(0);
static LAST_ELR: AtomicU64 = AtomicU64::new(0);
static LAST_FAR: AtomicU64 = AtomicU64::new(0);
static LAST_UNHANDLED_KIND: AtomicU64 = AtomicU64::new(u64::MAX);

core::arch::global_asm!(
    r#"
    .section .text
    .balign 2048
    .global __flx_aarch64_el1_vectors
__flx_aarch64_el1_vectors:
    .macro vector_entry handler
        b \handler
        .space 124
    .endm

    // Current EL using SP0.
    vector_entry __flx_aarch64_sync_entry
    vector_entry __flx_aarch64_irq_entry
    vector_entry __flx_aarch64_fiq_entry
    vector_entry __flx_aarch64_serror_entry
    // Current EL using SPx.
    vector_entry __flx_aarch64_sync_entry
    vector_entry __flx_aarch64_irq_entry
    vector_entry __flx_aarch64_fiq_entry
    vector_entry __flx_aarch64_serror_entry
    // Lower EL using AArch64.
    vector_entry __flx_aarch64_sync_entry
    vector_entry __flx_aarch64_irq_entry
    vector_entry __flx_aarch64_fiq_entry
    vector_entry __flx_aarch64_serror_entry
    // Lower EL using AArch32.
    vector_entry __flx_aarch64_sync_entry
    vector_entry __flx_aarch64_irq_entry
    vector_entry __flx_aarch64_fiq_entry
    vector_entry __flx_aarch64_serror_entry

    .global __flx_aarch64_sync_entry
__flx_aarch64_sync_entry:
    // Preserve every general-purpose register around the Rust diagnostic handler.
    sub sp, sp, #256
    stp x0,  x1,  [sp, #0]
    stp x2,  x3,  [sp, #16]
    stp x4,  x5,  [sp, #32]
    stp x6,  x7,  [sp, #48]
    stp x8,  x9,  [sp, #64]
    stp x10, x11, [sp, #80]
    stp x12, x13, [sp, #96]
    stp x14, x15, [sp, #112]
    stp x16, x17, [sp, #128]
    stp x18, x19, [sp, #144]
    stp x20, x21, [sp, #160]
    stp x22, x23, [sp, #176]
    stp x24, x25, [sp, #192]
    stp x26, x27, [sp, #208]
    stp x28, x29, [sp, #224]
    str x30, [sp, #240]
    bl __flx_aarch64_sync_dispatch
    ldp x0,  x1,  [sp, #0]
    ldp x2,  x3,  [sp, #16]
    ldp x4,  x5,  [sp, #32]
    ldp x6,  x7,  [sp, #48]
    ldp x8,  x9,  [sp, #64]
    ldp x10, x11, [sp, #80]
    ldp x12, x13, [sp, #96]
    ldp x14, x15, [sp, #112]
    ldp x16, x17, [sp, #128]
    ldp x18, x19, [sp, #144]
    ldp x20, x21, [sp, #160]
    ldp x22, x23, [sp, #176]
    ldp x24, x25, [sp, #192]
    ldp x26, x27, [sp, #208]
    ldp x28, x29, [sp, #224]
    ldr x30, [sp, #240]
    add sp, sp, #256
    eret

    // IRQ, FIQ and SError have distinct vector entries and diagnostics. They are
    // unexpected during early bring-up, so they are recorded and stopped, not resumed.
    .global __flx_aarch64_irq_entry
__flx_aarch64_irq_entry:
    sub sp, sp, #256
    stp x0,  x1,  [sp, #0]
    stp x2,  x3,  [sp, #16]
    stp x4,  x5,  [sp, #32]
    stp x6,  x7,  [sp, #48]
    stp x8,  x9,  [sp, #64]
    stp x10, x11, [sp, #80]
    stp x12, x13, [sp, #96]
    stp x14, x15, [sp, #112]
    stp x16, x17, [sp, #128]
    stp x18, x19, [sp, #144]
    stp x20, x21, [sp, #160]
    stp x22, x23, [sp, #176]
    stp x24, x25, [sp, #192]
    stp x26, x27, [sp, #208]
    stp x28, x29, [sp, #224]
    str x30, [sp, #240]
    bl __flx_aarch64_irq_dispatch
    ldp x0,  x1,  [sp, #0]
    ldp x2,  x3,  [sp, #16]
    ldp x4,  x5,  [sp, #32]
    ldp x6,  x7,  [sp, #48]
    ldp x8,  x9,  [sp, #64]
    ldp x10, x11, [sp, #80]
    ldp x12, x13, [sp, #96]
    ldp x14, x15, [sp, #112]
    ldp x16, x17, [sp, #128]
    ldp x18, x19, [sp, #144]
    ldp x20, x21, [sp, #160]
    ldp x22, x23, [sp, #176]
    ldp x24, x25, [sp, #192]
    ldp x26, x27, [sp, #208]
    ldp x28, x29, [sp, #224]
    ldr x30, [sp, #240]
    add sp, sp, #256
    eret
    .global __flx_aarch64_fiq_entry
__flx_aarch64_fiq_entry:
    mov x0, #2
    b __flx_aarch64_unhandled_dispatch_entry
    .global __flx_aarch64_serror_entry
__flx_aarch64_serror_entry:
    mov x0, #3
    b __flx_aarch64_unhandled_dispatch_entry
__flx_aarch64_unhandled_dispatch_entry:
    bl __flx_aarch64_unhandled_dispatch
1:
    wfe
    b 1b
    "#
);

extern "C" {
    static __flx_aarch64_el1_vectors: u8;
}

/// Installs the aligned EL1 vector table and leaves IRQ/FIQ/SError/debug masked.
///
/// # Safety
/// Must be called from EL1 with a valid writable kernel stack. The vector table must remain
/// mapped and executable for the lifetime of the kernel.
pub unsafe fn install_el1_vectors() -> Result<(), u8> {
    let level = super::current_exception_level();
    if level != 1 {
        return Err(level);
    }

    // Mask all exception classes before publishing the vector table.
    unsafe {
        core::arch::asm!("msr daifset, #0xf", options(nomem, nostack, preserves_flags));
    }

    let address = core::ptr::addr_of!(__flx_aarch64_el1_vectors) as u64;
    if address as usize % VECTOR_TABLE_ALIGNMENT != 0 {
        return Err(level);
    }

    // SAFETY: the linked vector table is 2 KiB aligned and remains in the image.
    unsafe {
        core::arch::asm!(
            "msr VBAR_EL1, {address}",
            "isb",
            address = in(reg) address,
            options(nostack, preserves_flags)
        );
    }
    Ok(())
}

/// Enables only IRQ exceptions after the GIC and IRQ handler are ready.
pub unsafe fn enable_irq() {
    // SAFETY: caller guarantees an installed vector table and initialized interrupt controller.
    unsafe { core::arch::asm!("msr daifclr, #2", options(nomem, nostack, preserves_flags)); }
}

/// Triggers a controlled BRK exception; the sync handler advances ELR_EL1 and resumes here.
pub fn trigger_controlled_breakpoint() {
    // SAFETY: the exception vector table must have been installed before this call.
    unsafe { core::arch::asm!("brk #0xf00", options(nostack)); }
}

/// Whether the controlled BRK was received and returned from successfully.
pub fn controlled_exception_seen() -> bool {
    CONTROLLED_EXCEPTION_SEEN.load(Ordering::Acquire)
}

pub fn last_exception_diagnostics() -> (u64, u64, u64, u64) {
    (
        LAST_ESR.load(Ordering::Relaxed),
        LAST_ELR.load(Ordering::Relaxed),
        LAST_FAR.load(Ordering::Relaxed),
        LAST_UNHANDLED_KIND.load(Ordering::Relaxed),
    )
}

#[no_mangle]
extern "C" fn __flx_aarch64_sync_dispatch() {
    let esr: u64;
    let elr: u64;
    let far: u64;
    unsafe {
        core::arch::asm!("mrs {value}, ESR_EL1", value = out(reg) esr, options(nomem, nostack, preserves_flags));
        core::arch::asm!("mrs {value}, ELR_EL1", value = out(reg) elr, options(nomem, nostack, preserves_flags));
        core::arch::asm!("mrs {value}, FAR_EL1", value = out(reg) far, options(nomem, nostack, preserves_flags));
    }
    LAST_ESR.store(esr, Ordering::Relaxed);
    LAST_ELR.store(elr, Ordering::Relaxed);
    LAST_FAR.store(far, Ordering::Relaxed);

    let exception_class = (esr >> 26) & 0x3f;
    let immediate = esr & 0xffff;
    if exception_class == ESR_EC_BRK64 && immediate == CONTROLLED_BRK_IMMEDIATE {
        // Resume after the 32-bit BRK instruction. No other synchronous exception is resumed.
        unsafe {
            core::arch::asm!(
                "add {next}, {current}, #4",
                "msr ELR_EL1, {next}",
                "isb",
                current = in(reg) elr,
                next = out(reg) _,
                options(nostack, preserves_flags)
            );
        }
        CONTROLLED_EXCEPTION_SEEN.store(true, Ordering::Release);
        return;
    }

    loop {
        unsafe { core::arch::asm!("wfe", options(nomem, nostack)); }
    }
}

#[no_mangle]
extern "C" fn __flx_aarch64_irq_dispatch() {
    let raw_iar = super::gic::acknowledge_group1();
    let interrupt_id = super::gic::interrupt_id(raw_iar);
    if interrupt_id == 1023 {
        // A spurious IAR must not cause an immediate exception-return/re-entry loop.
        super::timer::handle_interrupt(interrupt_id);
        mask_irq_on_exception_return();
        return;
    }

    super::timer::handle_interrupt(interrupt_id);
    super::gic::end_group1(raw_iar);

    if interrupt_id != super::gic::physical_timer_ppi() {
        // Keep early bring-up diagnosable: EOI the unexpected source and mask IRQ
        // in the saved PSTATE so the smoke test can report the offending INTID.
        LAST_UNHANDLED_KIND.store(interrupt_id as u64, Ordering::Relaxed);
        mask_irq_on_exception_return();
    }
}

#[inline]
fn mask_irq_on_exception_return() {
    // SPSR_EL1 is the PSTATE restored by ERET. Setting DAIF.I here prevents a
    // persistent unexpected source from immediately re-entering the IRQ vector.
    let mut saved = 0u64;
    unsafe {
        core::arch::asm!(
            "mrs {saved}, SPSR_EL1",
            "orr {saved}, {saved}, #0x80",
            "msr SPSR_EL1, {saved}",
            saved = inout(reg) saved,
            options(nostack, preserves_flags)
        );
    }
}

#[no_mangle]
extern "C" fn __flx_aarch64_unhandled_dispatch(kind: u64) -> ! {
    let esr: u64;
    let elr: u64;
    let far: u64;
    unsafe {
        core::arch::asm!("mrs {value}, ESR_EL1", value = out(reg) esr, options(nomem, nostack, preserves_flags));
        core::arch::asm!("mrs {value}, ELR_EL1", value = out(reg) elr, options(nomem, nostack, preserves_flags));
        core::arch::asm!("mrs {value}, FAR_EL1", value = out(reg) far, options(nomem, nostack, preserves_flags));
    }
    LAST_ESR.store(esr, Ordering::Relaxed);
    LAST_ELR.store(elr, Ordering::Relaxed);
    LAST_FAR.store(far, Ordering::Relaxed);
    LAST_UNHANDLED_KIND.store(kind, Ordering::Relaxed);
    loop {
        unsafe { core::arch::asm!("wfe", options(nomem, nostack)); }
    }
}
