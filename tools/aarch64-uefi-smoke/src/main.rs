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
    <arch::aarch64::AArch64 as CpuArchitecture>::halt_forever()
}

#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    serial_write(b"FLX AARCH64 UEFI BOOT: PANIC\r\n");
    loop {
        unsafe { core::arch::asm!("wfe", options(nomem, nostack)); }
    }
}
