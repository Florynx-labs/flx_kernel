//! Legacy 8259 Programmable Interrupt Controller (PIC) driver.
//!
//! The two cascaded PICs are remapped so that IRQ 0..=15 arrive on vectors 32..=47
//! (vectors 0..=31 are reserved for CPU exceptions). All lines start masked.
//!
//! STATUS: the Local APIC / IO-APIC are NOT IMPLEMENTED yet; they are required for SMP
//! (v1.0.0) and will replace this driver on multicore systems.

use crate::arch::x86_64::interrupts::IRQ_BASE;
use crate::arch::x86_64::port::Port;
use crate::sync::Spinlock;

const PIC1_COMMAND: u16 = 0x20;
const PIC1_DATA: u16 = 0x21;
const PIC2_COMMAND: u16 = 0xA0;
const PIC2_DATA: u16 = 0xA1;

const CMD_EOI: u8 = 0x20;
const CMD_READ_ISR: u8 = 0x0B;

struct Pics {
    cmd1: Port<u8>,
    data1: Port<u8>,
    cmd2: Port<u8>,
    data2: Port<u8>,
    /// Cached interrupt masks (bit set = line masked).
    mask1: u8,
    mask2: u8,
}

static PICS: Spinlock<Pics> = Spinlock::new(Pics {
    cmd1: Port::new(PIC1_COMMAND),
    data1: Port::new(PIC1_DATA),
    cmd2: Port::new(PIC2_COMMAND),
    data2: Port::new(PIC2_DATA),
    mask1: 0xFF,
    mask2: 0xFF,
});

/// Short delay between PIC commands (write to the unused POST port 0x80).
fn io_wait() {
    let mut p = Port::<u8>::new(0x80);
    // SAFETY: Port 0x80 is the conventional "debug/POST" port; writing is harmless.
    unsafe { p.write(0) };
}

/// Remaps the PICs to vectors 32..=47 and masks every line.
pub fn init() {
    let mut p = PICS.lock();
    // SAFETY: Standard ICW1..ICW4 initialization sequence of the 8259 PIC pair.
    unsafe {
        p.cmd1.write(0x11); // ICW1: init + ICW4 needed
        io_wait();
        p.cmd2.write(0x11);
        io_wait();
        p.data1.write(IRQ_BASE); // ICW2: master vector offset
        io_wait();
        p.data2.write(IRQ_BASE + 8); // ICW2: slave vector offset
        io_wait();
        p.data1.write(4); // ICW3: slave on IRQ2
        io_wait();
        p.data2.write(2); // ICW3: slave cascade identity
        io_wait();
        p.data1.write(0x01); // ICW4: 8086 mode
        io_wait();
        p.data2.write(0x01);
        io_wait();

        p.mask1 = 0xFF;
        p.mask2 = 0xFF;
        p.data1.write(0xFF);
        p.data2.write(0xFF);
    }
}

/// Unmasks the given IRQ line (0..=15). Unmasking a slave line also unmasks the cascade (IRQ2).
pub fn unmask(irq: u8) {
    let mut p = PICS.lock();
    // SAFETY: Writing the PIC interrupt mask registers.
    unsafe {
        if irq < 8 {
            p.mask1 &= !(1 << irq);
        } else {
            p.mask2 &= !(1 << (irq - 8));
            p.mask1 &= !(1 << 2);
        }
        let (m1, m2) = (p.mask1, p.mask2);
        p.data1.write(m1);
        p.data2.write(m2);
    }
}

/// Masks the given IRQ line (0..=15).
pub fn mask(irq: u8) {
    let mut p = PICS.lock();
    // SAFETY: Writing the PIC interrupt mask registers.
    unsafe {
        if irq < 8 {
            p.mask1 |= 1 << irq;
        } else {
            p.mask2 |= 1 << (irq - 8);
        }
        let (m1, m2) = (p.mask1, p.mask2);
        p.data1.write(m1);
        p.data2.write(m2);
    }
}

/// Sends End-Of-Interrupt for `irq`.
pub fn end_of_interrupt(irq: u8) {
    let mut p = PICS.lock();
    // SAFETY: EOI command writes.
    unsafe {
        if irq >= 8 {
            p.cmd2.write(CMD_EOI);
        }
        p.cmd1.write(CMD_EOI);
    }
}

/// Returns `true` if `irq` is a spurious IRQ 7 / 15 (line not set in the In-Service Register).
///
/// For a spurious IRQ 15 the master still needs an EOI (it saw the cascade line); this is
/// handled here. A spurious IRQ 7 must not be acknowledged at all.
pub fn is_spurious(irq: u8) -> bool {
    if irq != 7 && irq != 15 {
        return false;
    }
    let mut p = PICS.lock();
    // SAFETY: OCW3 read of the In-Service Register.
    unsafe {
        if irq == 7 {
            p.cmd1.write(CMD_READ_ISR);
            p.cmd1.read() & 0x80 == 0
        } else {
            p.cmd2.write(CMD_READ_ISR);
            if p.cmd2.read() & 0x80 == 0 {
                p.cmd1.write(CMD_EOI);
                true
            } else {
                false
            }
        }
    }
}
