//! 8253/8254 Programmable Interval Timer (PIT) — system tick source.
//!
//! Channel 0 is programmed as a rate generator at [`TICK_HZ`] and delivers IRQ 0.
//!
//! STATUS: LAPIC timer / HPET / TSC calibration are NOT IMPLEMENTED yet.

use core::sync::atomic::{AtomicU64, Ordering};

use crate::arch::x86_64::interrupts::{register_handler, InterruptFrame, IRQ_BASE};
use crate::arch::x86_64::port::Port;
use crate::drivers::pic;

/// PIT input clock (Hz).
const PIT_BASE_HZ: u32 = 1_193_182;
/// Kernel tick frequency (Hz).
pub const TICK_HZ: u32 = 100;

static TICKS: AtomicU64 = AtomicU64::new(0);

fn on_tick(_frame: &mut InterruptFrame) {
    TICKS.fetch_add(1, Ordering::Relaxed);
}

/// Programs channel 0 at [`TICK_HZ`], registers the IRQ 0 handler and unmasks the line.
pub fn init() {
    let divisor = (PIT_BASE_HZ / TICK_HZ) as u16;
    let mut command = Port::<u8>::new(0x43);
    let mut channel0 = Port::<u8>::new(0x40);
    // SAFETY: Standard PIT programming: channel 0, lobyte/hibyte access, mode 2 (rate generator).
    unsafe {
        command.write(0x34);
        channel0.write((divisor & 0xFF) as u8);
        channel0.write((divisor >> 8) as u8);
    }
    register_handler(IRQ_BASE, on_tick);
    pic::unmask(0);
}

/// Number of timer ticks since the timer was started.
pub fn ticks() -> u64 {
    TICKS.load(Ordering::Relaxed)
}

/// Milliseconds elapsed since the timer was started.
pub fn uptime_ms() -> u64 {
    ticks() * 1000 / TICK_HZ as u64
}
