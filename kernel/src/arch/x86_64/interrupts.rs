//! Minimal x86_64 Interrupt Descriptor Table (IDT).
//!
//! Every vector currently targets a fail-stop handler. This prevents an
//! unhandled exception from cascading into a triple fault, but does not yet
//! decode exception frames or resume execution. Interrupt sources remain
//! disabled until dedicated handlers and controller support are implemented.

use core::arch::{asm, global_asm};

const IDT_ENTRIES: usize = 256;
const INTERRUPT_GATE_FLAGS: u8 = 0x8E;

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct IdtEntry {
    offset_low: u16,
    selector: u16,
    ist: u8,
    flags: u8,
    offset_middle: u16,
    offset_high: u32,
    reserved: u32,
}

impl IdtEntry {
    const MISSING: Self = Self {
        offset_low: 0,
        selector: 0,
        ist: 0,
        flags: 0,
        offset_middle: 0,
        offset_high: 0,
        reserved: 0,
    };

    fn interrupt_gate(handler: u64, selector: u16) -> Self {
        Self {
            offset_low: handler as u16,
            selector,
            ist: 0,
            flags: INTERRUPT_GATE_FLAGS,
            offset_middle: (handler >> 16) as u16,
            offset_high: (handler >> 32) as u32,
            reserved: 0,
        }
    }
}

#[repr(C, packed)]
struct IdtPointer {
    limit: u16,
    base: u64,
}

static mut IDT: [IdtEntry; IDT_ENTRIES] = [IdtEntry::MISSING; IDT_ENTRIES];

global_asm!(
    ".global flx_unhandled_interrupt",
    ".type flx_unhandled_interrupt, @function",
    "flx_unhandled_interrupt:",
    "cli",
    "2:",
    "hlt",
    "jmp 2b",
);

extern "C" {
    fn flx_unhandled_interrupt() -> !;
}

/// Installs a baseline IDT containing 256 fail-stop interrupt gates.
///
/// # Safety
/// Must run during single-core early boot, before enabling interrupts or
/// starting other CPUs. All vectors intentionally halt the current CPU until
/// dedicated exception and hardware-interrupt handlers are implemented.
pub unsafe fn init() {
    let mut code_selector: u16;
    asm!(
        "mov {selector:x}, cs",
        selector = out(reg) code_selector,
        options(nomem, nostack, preserves_flags)
    );

    let handler = flx_unhandled_interrupt as usize as u64;
    let table = core::ptr::addr_of_mut!(IDT).cast::<IdtEntry>();

    for vector in 0..IDT_ENTRIES {
        // SAFETY: this is the exclusive early-boot initialization of the static
        // IDT; no other CPU or interrupt handler can access it yet.
        unsafe {
            core::ptr::write(
                table.add(vector),
                IdtEntry::interrupt_gate(handler, code_selector),
            );
        }
    }

    let pointer = IdtPointer {
        limit: (core::mem::size_of::<[IdtEntry; IDT_ENTRIES]>() - 1) as u16,
        base: core::ptr::addr_of!(IDT) as u64,
    };

    // SAFETY: pointer references the static IDT, whose lifetime is the kernel's
    // lifetime, and its entries have been initialized above.
    unsafe {
        asm!(
            "lidt [{}]",
            in(reg) &pointer,
            options(readonly, nostack, preserves_flags)
        );
    }
}
