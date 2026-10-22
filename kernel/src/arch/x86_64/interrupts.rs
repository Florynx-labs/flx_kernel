//! Interrupt and exception entry/dispatch for x86_64.
//!
//! Stable Rust has no `extern "x86-interrupt"` ABI, so every vector gets a tiny assembly
//! stub that normalizes the stack (pushes a dummy error code when the CPU does not),
//! pushes the vector number and all general-purpose registers, then calls
//! [`flx_interrupt_dispatch`] with a pointer to an [`InterruptFrame`]. The same frame
//! layout will later be reused for context switching (scheduler, v0.4.0).

use core::arch::global_asm;
use core::sync::atomic::{AtomicUsize, Ordering};

use super::cpu;
use crate::drivers::pic;

/// Saved CPU state at interrupt entry. Field order matches the push order in the stubs
/// (lowest address first); the last five fields are pushed by the CPU itself.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct InterruptFrame {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rbp: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rbx: u64,
    pub rax: u64,
    pub vector: u64,
    /// Hardware error code, or 0 for vectors that do not push one.
    pub error_code: u64,
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

/// Handler signature for registered vectors.
pub type Handler = fn(&mut InterruptFrame);

/// First vector used for hardware IRQs (remapped 8259 PIC).
pub const IRQ_BASE: u8 = 32;

static HANDLERS: [AtomicUsize; 256] = [const { AtomicUsize::new(0) }; 256];

/// Registers a handler for `vector`. Handlers run with interrupts disabled.
pub fn register_handler(vector: u8, handler: Handler) {
    HANDLERS[vector as usize].store(handler as usize, Ordering::Release);
}

fn handler_for(vector: usize) -> Option<Handler> {
    match HANDLERS[vector].load(Ordering::Acquire) {
        0 => None,
        // SAFETY: Only values stored by `register_handler` (valid `Handler` pointers) are non-zero.
        ptr => Some(unsafe { core::mem::transmute::<usize, Handler>(ptr) }),
    }
}

// ---------------------------------------------------------------------------------------
// Assembly stubs (Intel syntax, the Rust default for x86).
//
// Vectors that push an error code: 8, 10, 11, 12, 13, 14, 17, 21, 29, 30.
// ---------------------------------------------------------------------------------------
global_asm!(
    r#"
    .altmacro

    .macro FLX_STUB n
    .balign 16
    flx_isr_\n:
    .if (\n == 8) || (\n == 10) || (\n == 11) || (\n == 12) || (\n == 13) || (\n == 14) || (\n == 17) || (\n == 21) || (\n == 29) || (\n == 30)
    .else
        push 0
    .endif
        push \n
        jmp flx_isr_common
    .endm

    .macro FLX_TABLE_ENTRY n
        .quad flx_isr_\n
    .endm

    .text
    .set flx_vec, 0
    .rept 256
        FLX_STUB %flx_vec
        .set flx_vec, flx_vec + 1
    .endr

    .balign 16
    flx_isr_common:
        push rax
        push rbx
        push rcx
        push rdx
        push rbp
        push rsi
        push rdi
        push r8
        push r9
        push r10
        push r11
        push r12
        push r13
        push r14
        push r15
        mov rdi, rsp
        cld
        call {dispatch}
        pop r15
        pop r14
        pop r13
        pop r12
        pop r11
        pop r10
        pop r9
        pop r8
        pop rdi
        pop rsi
        pop rbp
        pop rdx
        pop rcx
        pop rbx
        pop rax
        add rsp, 16
        iretq

    .section .rodata.flx_isr_table,"a"
    .balign 8
    .global flx_isr_table
    flx_isr_table:
    .set flx_vec, 0
    .rept 256
        FLX_TABLE_ENTRY %flx_vec
        .set flx_vec, flx_vec + 1
    .endr

    .text
    .noaltmacro
    "#,
    dispatch = sym flx_interrupt_dispatch,
);

extern "C" {
    static flx_isr_table: [u64; 256];
}

/// Address of the assembly stub for `vector` (used by the IDT).
pub fn isr_stub_address(vector: usize) -> u64 {
    // SAFETY: `flx_isr_table` is a read-only table of 256 stub addresses emitted above.
    unsafe { flx_isr_table[vector] }
}

// ---------------------------------------------------------------------------------------
// Dispatch
// ---------------------------------------------------------------------------------------

const EXCEPTION_NAMES: [&str; 32] = [
    "Divide Error (#DE)",
    "Debug (#DB)",
    "Non-Maskable Interrupt",
    "Breakpoint (#BP)",
    "Overflow (#OF)",
    "Bound Range Exceeded (#BR)",
    "Invalid Opcode (#UD)",
    "Device Not Available (#NM)",
    "Double Fault (#DF)",
    "Coprocessor Segment Overrun",
    "Invalid TSS (#TS)",
    "Segment Not Present (#NP)",
    "Stack-Segment Fault (#SS)",
    "General Protection Fault (#GP)",
    "Page Fault (#PF)",
    "Reserved",
    "x87 Floating-Point (#MF)",
    "Alignment Check (#AC)",
    "Machine Check (#MC)",
    "SIMD Floating-Point (#XM)",
    "Virtualization (#VE)",
    "Control Protection (#CP)",
    "Reserved",
    "Reserved",
    "Reserved",
    "Reserved",
    "Reserved",
    "Reserved",
    "Hypervisor Injection (#HV)",
    "VMM Communication (#VC)",
    "Security (#SX)",
    "Reserved",
];

/// Single Rust entry point for every interrupt and exception.
extern "C" fn flx_interrupt_dispatch(frame: *mut InterruptFrame) {
    // SAFETY: The stub passes a pointer to the frame it just built on the current stack.
    let frame = unsafe { &mut *frame };
    let vector = frame.vector as usize;

    if vector < 32 {
        handle_exception(frame);
    } else if (IRQ_BASE as usize..IRQ_BASE as usize + 16).contains(&vector) {
        let irq = (vector - IRQ_BASE as usize) as u8;
        // Spurious IRQ 7 / 15: acknowledge nothing (except the master for IRQ 15).
        if pic::is_spurious(irq) {
            return;
        }
        if let Some(handler) = handler_for(vector) {
            handler(frame);
        }
        pic::end_of_interrupt(irq);
    } else if let Some(handler) = handler_for(vector) {
        handler(frame);
    } else {
        crate::kwarn!("Unhandled interrupt vector {}", vector);
    }
}

fn handle_exception(frame: &mut InterruptFrame) {
    // A registered handler takes precedence (used by tests and, later, by the VM subsystem).
    if let Some(handler) = handler_for(frame.vector as usize) {
        handler(frame);
        return;
    }

    match frame.vector {
        3 => {
            crate::kwarn!("Breakpoint at RIP={:#018x}", frame.rip);
        }
        _ => {
            dump_frame(frame);
            let name = EXCEPTION_NAMES[frame.vector as usize];
            panic!("unrecoverable CPU exception: {}", name);
        }
    }
}

fn dump_frame(f: &InterruptFrame) {
    let name = EXCEPTION_NAMES[f.vector as usize];
    crate::kerror!("EXCEPTION {} (vector {}), error code {:#x}", name, f.vector, f.error_code);
    if f.vector == 14 {
        let e = f.error_code;
        crate::kerror!(
            "  fault address {:#018x} [{}{}{}{}]",
            cpu::read_cr2(),
            if e & 1 != 0 { "protection " } else { "not-present " },
            if e & 2 != 0 { "write " } else { "read " },
            if e & 4 != 0 { "user " } else { "kernel " },
            if e & 16 != 0 { "ifetch" } else { "" },
        );
    }
    crate::kerror!("  RIP={:#018x} CS={:#06x} RFLAGS={:#x}", f.rip, f.cs, f.rflags);
    crate::kerror!("  RSP={:#018x} SS={:#06x}", f.rsp, f.ss);
    crate::kerror!("  RAX={:#018x} RBX={:#018x} RCX={:#018x}", f.rax, f.rbx, f.rcx);
    crate::kerror!("  RDX={:#018x} RSI={:#018x} RDI={:#018x}", f.rdx, f.rsi, f.rdi);
    crate::kerror!("  RBP={:#018x} R8 ={:#018x} R9 ={:#018x}", f.rbp, f.r8, f.r9);
}

/// Triggers a breakpoint exception (`int3`); used as a boot-time IDT self-test.
pub fn trigger_breakpoint() {
    // SAFETY: `int3` raises #BP, which our handler logs and resumes from.
    unsafe {
        core::arch::asm!("int3", options(nomem, nostack));
    }
}
