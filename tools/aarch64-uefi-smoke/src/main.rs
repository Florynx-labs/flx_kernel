#![no_std]
#![no_main]

// First-stage AArch64 UEFI kernel bring-up image for QEMU virt.
// This deliberately does not claim to initialize the full FLX kernel subsystems.

use core::ffi::c_void;
use core::panic::PanicInfo;
use core::ptr::{read_volatile, write_volatile};

// Compile the production AArch64 CPU contract and primitives into the firmware image.
#[path = "../../../kernel/src/arch/interface.rs"]
pub mod arch_interface;
#[path = "../../../kernel/src/arch/aarch64/mod.rs"]
pub mod arch_aarch64;

pub mod arch {
    pub use crate::arch_aarch64 as aarch64;
    pub use crate::arch_interface as interface;
}

use arch::interface::CpuArchitecture;

const PL011_BASE: usize = 0x0900_0000;
const UARTDR: usize = PL011_BASE;
const UARTFR: usize = PL011_BASE + 0x18;
const UARTIBRD: usize = PL011_BASE + 0x24;
const UARTFBRD: usize = PL011_BASE + 0x28;
const UARTLCR_H: usize = PL011_BASE + 0x2c;
const UARTCR: usize = PL011_BASE + 0x30;
const UARTICR: usize = PL011_BASE + 0x44;

#[inline]
unsafe fn mmio_write(address: usize, value: u32) {
    // SAFETY: The caller supplies a valid QEMU virt PL011 MMIO register address.
    unsafe { write_volatile(address as *mut u32, value) };
}

#[inline]
unsafe fn mmio_read(address: usize) -> u32 {
    // SAFETY: The caller supplies a valid QEMU virt PL011 MMIO register address.
    unsafe { read_volatile(address as *const u32) }
}

fn serial_init() {
    // QEMU virt exposes a PL011 UART at 0x0900_0000 with a 24 MHz reference clock.
    // Divisor 13 + fraction 1 gives approximately 115200 baud.
    unsafe {
        mmio_write(UARTCR, 0);
        mmio_write(UARTICR, 0x7ff);
        mmio_write(UARTIBRD, 13);
        mmio_write(UARTFBRD, 1);
        mmio_write(UARTLCR_H, 0x70); // 8-bit words, FIFO enabled.
        mmio_write(UARTCR, 0x301);   // UART enabled, TX enabled, RX enabled.
    }
}

fn serial_write_hex(label: &[u8], value: u64) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    serial_write(label);
    for shift in (0..16).rev() {
        let digit = ((value >> (shift * 4)) & 0xf) as usize;
        serial_write(&[HEX[digit]]);
    }
    serial_write(b"\r\n");
}

fn serial_write(bytes: &[u8]) {
    for byte in bytes {
        unsafe {
            while mmio_read(UARTFR) & (1 << 5) != 0 {
                core::hint::spin_loop();
            }
            mmio_write(UARTDR, *byte as u32);
        }
    }
}

#[no_mangle]
pub extern "efiapi" fn efi_main(
    _image_handle: usize,
    _system_table: *mut c_void,
) -> usize {
    serial_init();
    serial_write(b"FLX AARCH64 UEFI BOOT: PASS\r\n");

    // Install the production EL1 vector table while all exception classes remain masked.
    if let Err(level) = unsafe { arch::aarch64::exceptions::install_el1_vectors() } {
        serial_write(b"FLX AARCH64 EXCEPTION: WRONG EL\r\n");
        let _ = level;
        <arch::aarch64::AArch64 as CpuArchitecture>::halt_forever();
    }

    arch::aarch64::exceptions::trigger_controlled_breakpoint();
    let (esr, elr, far, unhandled_kind) =
        arch::aarch64::exceptions::last_exception_diagnostics();
    serial_write_hex(b"ESR_EL1=0x", esr);
    serial_write_hex(b"ELR_EL1=0x", elr);
    serial_write_hex(b"FAR_EL1=0x", far);
    let expected_brk = ((esr >> 26) & 0x3f) == 0x3c && (esr & 0xffff) == 0x0f00;
    if arch::aarch64::exceptions::controlled_exception_seen()
        && expected_brk
        && elr != 0
        && unhandled_kind == u64::MAX
    {
        serial_write(b"FLX AARCH64 EXCEPTION: PASS\r\n");
    } else {
        serial_write(b"FLX AARCH64 EXCEPTION: FAIL\r\n");
        <arch::aarch64::AArch64 as CpuArchitecture>::halt_forever();
    }

    if unsafe { arch::aarch64::timer::init_periodic(10) }.is_err() {
        serial_write(b"FLX AARCH64 TIMER: INIT FAIL\r\n");
        <arch::aarch64::AArch64 as CpuArchitecture>::halt_forever();
    }
    let deadline = arch::aarch64::timer::counter()
        .saturating_add(arch::aarch64::timer::frequency().saturating_mul(2));
    while arch::aarch64::timer::ticks() < 3
        && arch::aarch64::timer::counter() < deadline
    {
        // The configured EL1 physical timer IRQ wakes this WFI.
        unsafe { core::arch::asm!("wfi", options(nomem, nostack)); }
    }
    let ticks = arch::aarch64::timer::ticks();
    arch::aarch64::timer::stop();
    if ticks >= 3 {
        serial_write(b"FLX AARCH64 TIMER: PASS\r\n");
    } else {
        serial_write(b"FLX AARCH64 TIMER: FAIL\r\n");
    }
    <arch::aarch64::AArch64 as CpuArchitecture>::halt_forever()
}

#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    serial_write(b"FLX AARCH64 UEFI BOOT: PANIC\r\n");
    loop {
        unsafe { core::arch::asm!("wfe", options(nomem, nostack)); }
    }
}
