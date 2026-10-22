//! Interrupt Descriptor Table (IDT).
//!
//! All 256 vectors point to assembly stubs defined in [`super::interrupts`]; those stubs
//! build a uniform [`super::interrupts::InterruptFrame`] and call a single Rust dispatcher.

use core::arch::asm;
use core::mem::size_of;
use core::ptr::addr_of_mut;

use super::gdt;
use super::interrupts::isr_stub_address;

#[derive(Clone, Copy)]
#[repr(C)]
struct IdtEntry {
    offset_low: u16,
    selector: u16,
    /// Bits 0..=2: IST index (0 = none). Remaining bits must be zero.
    ist: u8,
    /// P | DPL | 0 | gate type.
    type_attr: u8,
    offset_mid: u16,
    offset_high: u32,
    zero: u32,
}

impl IdtEntry {
    const MISSING: Self = Self {
        offset_low: 0,
        selector: 0,
        ist: 0,
        type_attr: 0,
        offset_mid: 0,
        offset_high: 0,
        zero: 0,
    };

    /// Creates a 64-bit interrupt gate (interrupts are masked on entry).
    fn interrupt_gate(handler: u64, ist: u8, user_callable: bool) -> Self {
        Self {
            offset_low: handler as u16,
            selector: gdt::KERNEL_CODE_SELECTOR,
            ist,
            // 0x8E = present, DPL0, 64-bit interrupt gate; 0xEE = same with DPL3.
            type_attr: if user_callable { 0xEE } else { 0x8E },
            offset_mid: (handler >> 16) as u16,
            offset_high: (handler >> 32) as u32,
            zero: 0,
        }
    }
}

#[repr(C, align(16))]
struct Idt([IdtEntry; 256]);

static mut IDT: Idt = Idt([IdtEntry::MISSING; 256]);

#[repr(C, packed)]
struct DescriptorTablePointer {
    limit: u16,
    base: u64,
}

/// Fills the IDT with the assembly stubs and loads it with `lidt`.
///
/// Must be called after [`gdt::init`] (the gates reference the kernel code selector
/// and the IST slots configured in the TSS).
pub fn init() {
    // SAFETY: Called once on the boot CPU during early boot; we have exclusive access to
    // the static IDT and the stub addresses come from our own assembly table.
    unsafe {
        let idt = addr_of_mut!(IDT);
        for vector in 0..256usize {
            let ist = match vector {
                2 => gdt::NMI_IST,           // NMI
                8 => gdt::DOUBLE_FAULT_IST,  // #DF
                _ => 0,
            };
            // Breakpoint (#BP, int3) may be raised from user mode later on.
            let user_callable = vector == 3;
            (*idt).0[vector] =
                IdtEntry::interrupt_gate(isr_stub_address(vector), ist, user_callable);
        }

        let pointer = DescriptorTablePointer {
            limit: (size_of::<Idt>() - 1) as u16,
            base: idt as u64,
        };
        asm!(
            "lidt [{ptr}]",
            ptr = in(reg) &pointer,
            options(readonly, nostack, preserves_flags)
        );
    }
}
