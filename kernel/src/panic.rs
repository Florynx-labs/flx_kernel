//! Kernel panic handler for FLX Kernel.
//!
//! Provides diagnostic reporting over serial and framebuffer before disabling
//! interrupts and halting the CPU permanently.

use core::panic::PanicInfo;
use crate::arch::x86_64::cpu;
use crate::drivers::serial::SERIAL1;
use crate::framebuffer::{self, Color, FRAMEBUFFER};

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    // 1. Disable interrupts immediately — no further fault cascades.
    cpu::disable_interrupts();

    // 2. Force-unlock in case panic occurred inside a lock critical section.
    // SAFETY: Unrecoverable kernel panic; preventing deadlock takes priority over exclusion.
    unsafe {
        SERIAL1.force_unlock();
        FRAMEBUFFER.force_unlock();
    }

    // 3. Serial diagnostic banner
    crate::serial_println!();
    crate::serial_println!("\x1b[91;1m================================================================================");
    crate::serial_println!("                              FLX KERNEL PANIC");
    crate::serial_println!("================================================================================\x1b[0m");

    // 4. Framebuffer diagnostic banner
    framebuffer::set_color(Color::RED);
    crate::fb_println!();
    crate::fb_println!("================================================================================");
    crate::fb_println!("                              FLX KERNEL PANIC");
    crate::fb_println!("================================================================================");
    framebuffer::set_color(Color::WHITE);

    // 5. Location
    if let Some(location) = info.location() {
        crate::serial_println!("\x1b[93mLocation:\x1b[0m {}:{}:{}", location.file(), location.line(), location.column());
        framebuffer::set_color(Color::YELLOW);
        crate::fb_println!("Location: {}:{}:{}", location.file(), location.line(), location.column());
        framebuffer::set_color(Color::WHITE);
    } else {
        crate::serial_println!("\x1b[93mLocation:\x1b[0m <unknown>");
        crate::fb_println!("Location: <unknown>");
    }

    // 6. Message
    crate::serial_println!("\x1b[97;1mMessage:\x1b[0m  {}", info.message());
    crate::fb_println!("Message:  {}", info.message());

    // 7. CPU info
    let vendor = cpu::get_cpu_vendor();
    crate::serial_println!("\x1b[90mCPU:\x1b[0m      x86_64 ({})", vendor.as_str());
    crate::serial_println!("\x1b[91mStatus:   CPU HALTED.\x1b[0m");
    crate::serial_println!("================================================================================");

    framebuffer::set_color(Color::GRAY);
    crate::fb_println!("CPU:      x86_64 ({})", vendor.as_str());
    framebuffer::set_color(Color::RED);
    crate::fb_println!("Status:   CPU HALTED.");
    crate::fb_println!("================================================================================");

    // 8. Halt CPU — this function never returns.
    cpu::halt_loop();
}
