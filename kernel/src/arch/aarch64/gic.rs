//! GICv3 local interrupt-controller support for QEMU virt.
//!
//! This initial backend configures the boot CPU's redistributor and Group 1 system-register
//! interface. It intentionally does not claim SMP redistributor discovery or SPI routing.

use core::hint::spin_loop;
use core::ptr::{read_volatile, write_volatile};

const GICD_BASE: usize = 0x0800_0000;
const GICR_BASE: usize = 0x080a_0000;
const GICD_CTLR: usize = GICD_BASE;
const GICR_CTLR: usize = GICR_BASE;
const GICR_WAKER: usize = GICR_BASE + 0x14;
const GICR_IGROUPR0: usize = GICR_BASE + 0x10080;
const GICR_ICENABLER0: usize = GICR_BASE + 0x10180;
const GICR_ISENABLER0: usize = GICR_BASE + 0x10100;
const GICR_ICPENDR0: usize = GICR_BASE + 0x10280;
const GICR_ISPENDR0: usize = GICR_BASE + 0x10200;
const VIRTUAL_TIMER_PPI: u32 = 27;
const GICR_IPRIORITYR: usize = GICR_BASE + 0x10400;
const PHYSICAL_TIMER_PPI: u32 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GicError {
    RedistributorWakeTimeout,
    RedistributorWritePendingTimeout,
    DistributorWritePendingTimeout,
    PpiConfigurationFailed,
    DistributorConfigurationFailed,
    CpuInterfaceUnavailable,
}

#[inline]
unsafe fn read32(address: usize) -> u32 {
    // SAFETY: caller uses a documented GICv3 MMIO register.
    unsafe { read_volatile(address as *const u32) }
}

#[inline]
unsafe fn write32(address: usize, value: u32) {
    // SAFETY: caller uses a documented GICv3 MMIO register.
    unsafe { write_volatile(address as *mut u32, value) }
}

/// Initializes the QEMU virt GICv3 boot-CPU interface and enables the EL1 physical timer PPI.
///
/// # Safety
/// Must run at EL1 on QEMU virt with the GICv3 distributor/redistributor mapped at the
/// architectural QEMU virt addresses. The current implementation assumes CPU 0's redistributor.
pub unsafe fn init_physical_timer_ppi() -> Result<(), GicError> {
    // Wake the boot CPU redistributor.
    let waker = unsafe { read32(GICR_WAKER) } & !(1 << 1);
    unsafe { write32(GICR_WAKER, waker) };
    let mut awake = false;
    for _ in 0..1_000_000 {
        if unsafe { read32(GICR_WAKER) } & (1 << 2) == 0 {
            awake = true;
            break;
        }
        spin_loop();
    }
    if !awake {
        return Err(GicError::RedistributorWakeTimeout);
    }

    let ppi_mask = 1u32 << PHYSICAL_TIMER_PPI;
    let inherited_virtual_timer_mask = 1u32 << VIRTUAL_TIMER_PPI;
    unsafe {
        // UEFI may leave its virtual timer PPI enabled and pending. This kernel
        // owns the EL1 timer from this point, so retire that inherited source
        // before unmasking IRQ and use the physical timer PPI (INTID 30) only.
        write32(GICR_ICENABLER0, inherited_virtual_timer_mask);
        write32(GICR_ICPENDR0, inherited_virtual_timer_mask);
        // Configure the physical timer as Group 1 Non-secure, then enable its PPI.
        write32(GICR_IGROUPR0, read32(GICR_IGROUPR0) | ppi_mask);
        core::ptr::write_volatile((GICR_IPRIORITYR + PHYSICAL_TIMER_PPI as usize) as *mut u8, 0x80);
        core::arch::asm!("dsb sy", options(nostack, preserves_flags));
        write32(GICR_ISENABLER0, ppi_mask);
        core::arch::asm!("dsb sy", options(nostack, preserves_flags));
    }
    // QEMU completes redistributor SGI/PPI register writes synchronously. DSB above
    // orders them; GICR_CTLR.RWP is RAZ/WI on this implementation and must not be polled.

    // Read back the two PPI control bits. A silent/incorrect MMIO mapping must not
    // be mistaken for a successfully configured interrupt source.
    let group = unsafe { read32(GICR_IGROUPR0) };
    let enabled = unsafe { read32(GICR_ISENABLER0) };
    let inherited_virtual_timer_enabled =
        unsafe { read32(GICR_ISENABLER0) } & inherited_virtual_timer_mask != 0;
    let inherited_virtual_timer_pending =
        unsafe { read32(GICR_ISPENDR0) } & inherited_virtual_timer_mask != 0;
    if group & ppi_mask == 0
        || enabled & ppi_mask == 0
        || inherited_virtual_timer_enabled
        || inherited_virtual_timer_pending
    {
        return Err(GicError::PpiConfigurationFailed);
    }

    // Enable Group 1 Non-secure interrupts at the distributor and wait for completion.
    unsafe {
        write32(GICD_CTLR, read32(GICD_CTLR) | (1 << 1));
        core::arch::asm!("dsb sy", options(nostack, preserves_flags));
    }
    // QEMU's GICv3 distributor applies this control write synchronously; DSB above
    // provides completion ordering before the CPU interface is enabled.
    if unsafe { read32(GICD_CTLR) } & (1 << 1) == 0 {
        return Err(GicError::DistributorConfigurationFailed);
    }

    // Enable the EL1 system-register interface and permit Group 1 interrupts.
    unsafe {
        let sre: u64;
        core::arch::asm!(
            "mrs {sre}, ICC_SRE_EL1",
            "orr {sre}, {sre}, #1",
            "msr ICC_SRE_EL1, {sre}",
            "isb",
            "mrs {sre}, ICC_SRE_EL1",
            sre = out(reg) sre,
            options(nostack, preserves_flags)
        );
        if sre & 1 == 0 {
            return Err(GicError::CpuInterfaceUnavailable);
        }
        core::arch::asm!(
            "msr ICC_PMR_EL1, {pmr}",
            "msr ICC_BPR1_EL1, xzr",
            "msr ICC_IGRPEN1_EL1, {enable}",
            "isb",
            pmr = in(reg) 0xffu64,
            enable = in(reg) 1u64,
            options(nostack, preserves_flags)
        );
        core::arch::asm!("dsb sy", "isb", options(nostack, preserves_flags));
    }
    let group1_enabled: u64;
    unsafe { core::arch::asm!("mrs {value}, ICC_IGRPEN1_EL1", value = out(reg) group1_enabled, options(nomem, nostack, preserves_flags)); }
    if group1_enabled & 1 == 0 {
        return Err(GicError::CpuInterfaceUnavailable);
    }
    Ok(())
}

/// Acknowledge the highest-priority Group 1 interrupt; returns the raw IAR token.
pub fn acknowledge_group1() -> u32 {
    let iar: u64;
    // SAFETY: GIC system-register interface was enabled during initialization.
    unsafe { core::arch::asm!("mrs {value}, ICC_IAR1_EL1", value = out(reg) iar, options(nomem, nostack, preserves_flags)); }
    iar as u32
}

/// End the specified Group 1 interrupt using the raw IAR token.
pub fn end_group1(raw_iar: u32) {
    // SAFETY: raw_iar is the token returned by ICC_IAR1_EL1 for this active interrupt.
    unsafe {
        core::arch::asm!(
            "msr ICC_EOIR1_EL1, {iar}",
            "dsb sy",
            "isb",
            iar = in(reg) raw_iar as u64,
            options(nostack, preserves_flags)
        );
    }
}

#[inline]
pub fn interrupt_id(raw_iar: u32) -> u32 {
    raw_iar & 0x3ff
}

pub const fn physical_timer_ppi() -> u32 {
    PHYSICAL_TIMER_PPI
}

/// Captures the boot CPU GICv3 state relevant to physical timer delivery.
/// Values are returned in a stable order for early-boot serial diagnostics.
pub fn timer_diagnostics() -> [u64; 8] {
    let group_enable;
    let mut sre = 0u64;
    let mut pmr = 0u64;
    let mut group1_enable = 0u64;
    unsafe {
        core::arch::asm!("mrs {value}, ICC_SRE_EL1", value = out(reg) sre, options(nomem, nostack, preserves_flags));
        core::arch::asm!("mrs {value}, ICC_PMR_EL1", value = out(reg) pmr, options(nomem, nostack, preserves_flags));
        core::arch::asm!("mrs {value}, ICC_IGRPEN1_EL1", value = out(reg) group1_enable, options(nomem, nostack, preserves_flags));
        group_enable = read32(GICR_IGROUPR0) as u64;
    }
    [
        unsafe { read32(GICD_CTLR) } as u64,
        unsafe { read32(GICR_CTLR) } as u64,
        unsafe { read32(GICR_WAKER) } as u64,
        group_enable,
        unsafe { read32(GICR_ISENABLER0) } as u64,
        unsafe { read32(GICR_ISPENDR0) } as u64,
        sre,
        (pmr << 32) | group1_enable,
    ]
}
