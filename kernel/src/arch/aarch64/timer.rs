//! AArch64 EL1 physical Generic Timer support.

use core::sync::atomic::{AtomicU64, Ordering};

use super::gic;

static TICKS: AtomicU64 = AtomicU64::new(0);
static RELOAD_TICKS: AtomicU64 = AtomicU64::new(0);
static COUNTER_FREQUENCY: AtomicU64 = AtomicU64::new(0);
static LAST_INTERRUPT_ID: AtomicU64 = AtomicU64::new(u64::MAX);
static UNEXPECTED_INTERRUPTS: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerError {
    WrongExceptionLevel(u8),
    InvalidPeriod,
    Gic(gic::GicError),
}

#[inline]
fn read_counter_frequency() -> u64 {
    let value: u64;
    // SAFETY: CNTFRQ_EL0 is readable at EL1.
    unsafe { core::arch::asm!("mrs {value}, CNTFRQ_EL0", value = out(reg) value, options(nomem, nostack, preserves_flags)); }
    value
}

#[inline]
pub fn counter() -> u64 {
    let value: u64;
    // SAFETY: CNTVCT_EL0 is readable at EL1 on QEMU virt.
    unsafe { core::arch::asm!("mrs {value}, CNTVCT_EL0", value = out(reg) value, options(nomem, nostack, preserves_flags)); }
    value
}

#[inline]
fn program_next_tick(ticks: u64) {
    // SAFETY: EL1 physical timer registers are accessible from the kernel at EL1.
    unsafe {
        core::arch::asm!(
            "msr CNTP_TVAL_EL0, {ticks}",
            "msr CNTP_CTL_EL0, {enable}",
            "isb",
            ticks = in(reg) ticks,
            enable = in(reg) 1u64,
            options(nostack, preserves_flags)
        );
    }
}

/// Configures a periodic EL1 physical timer and enables its GICv3 PPI.
///
/// # Safety
/// Requires the EL1 exception vectors installed and QEMU virt GICv3 MMIO regions accessible.
pub unsafe fn init_periodic(period_ms: u64) -> Result<(), TimerError> {
    let level = super::current_exception_level();
    if level != 1 {
        return Err(TimerError::WrongExceptionLevel(level));
    }
    if period_ms == 0 {
        return Err(TimerError::InvalidPeriod);
    }

    super::disable_interrupts();
    let frequency = read_counter_frequency();
    if frequency == 0 {
        return Err(TimerError::InvalidPeriod);
    }
    let reload = ((frequency as u128) * (period_ms as u128) / 1000) as u64;
    if reload == 0 {
        return Err(TimerError::InvalidPeriod);
    }

    TICKS.store(0, Ordering::Release);
    RELOAD_TICKS.store(reload, Ordering::Release);
    COUNTER_FREQUENCY.store(frequency, Ordering::Release);

    // UEFI may leave its virtual timer running. The kernel takes ownership
    // of the EL1 timer interrupt path, so disable both sources before GIC setup.
    // SAFETY: EL1 timer control registers are accessible at EL1.
    unsafe {
        core::arch::asm!(
            "msr CNTV_CTL_EL0, xzr",
            "msr CNTP_CTL_EL0, xzr",
            "isb",
            options(nomem, nostack, preserves_flags)
        );
    }
    unsafe {
        gic::init_physical_timer_ppi().map_err(TimerError::Gic)?;
    }
    program_next_tick(reload);

    // VBAR_EL1 is installed and GICv3 is ready; only now unmask IRQ.
    unsafe { super::exceptions::enable_irq(); }
    Ok(())
}

/// Stops the physical timer and masks local IRQ delivery.
pub fn stop() {
    super::disable_interrupts();
    // SAFETY: EL1 physical timer control register is accessible from the kernel.
    unsafe { core::arch::asm!("msr CNTP_CTL_EL0, xzr", "isb", options(nomem, nostack, preserves_flags)); }
}

pub fn ticks() -> u64 {
    TICKS.load(Ordering::Acquire)
}

pub fn frequency() -> u64 {
    COUNTER_FREQUENCY.load(Ordering::Acquire)
}

pub(crate) fn handle_interrupt(interrupt_id: u32) {
    LAST_INTERRUPT_ID.store(interrupt_id as u64, Ordering::Release);
    if interrupt_id == gic::physical_timer_ppi() {
        TICKS.fetch_add(1, Ordering::AcqRel);
        let reload = RELOAD_TICKS.load(Ordering::Acquire);
        if reload != 0 {
            program_next_tick(reload);
        }
    } else {
        UNEXPECTED_INTERRUPTS.fetch_add(1, Ordering::AcqRel);
    }
}

pub fn last_interrupt_id() -> u64 { LAST_INTERRUPT_ID.load(Ordering::Acquire) }
pub fn unexpected_interrupts() -> u64 { UNEXPECTED_INTERRUPTS.load(Ordering::Acquire) }

/// Returns CNTFRQ_EL0, CNTVCT_EL0 and CNTP_CTL_EL0 for early-boot diagnostics.
pub fn hardware_diagnostics() -> [u64; 3] {
    let mut control = 0u64;
    // SAFETY: these architectural timer registers are readable at EL1.
    unsafe {
        core::arch::asm!("mrs {value}, CNTP_CTL_EL0", value = out(reg) control, options(nomem, nostack, preserves_flags));
    }
    [read_counter_frequency(), counter(), control]
}
