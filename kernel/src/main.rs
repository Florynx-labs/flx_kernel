//! FLX Kernel — Core Entry Point (Milestone v0.2.0)
//!
//! Developed by Florynx Labs.
//! High-performance, modular, secure general-purpose operating system kernel in Rust no_std.

#![no_std]
#![no_main]

pub mod arch;
pub mod boot;
pub mod drivers;
pub mod framebuffer;
pub mod logging;
pub mod mm;
pub mod sync;

// panic handler is in its own file and auto-linked as #[panic_handler]
mod panic;

use bootloader_api::config::Mapping;
use bootloader_api::{entry_point, BootInfo, BootloaderConfig};
use log::LevelFilter;

pub const KERNEL_VERSION: &str = "0.2.0-dev";

/// Bootloader configuration requesting dynamic physical memory mapping.
pub static BOOTLOADER_CONFIG: BootloaderConfig = {
    let mut config = BootloaderConfig::new_default();
    config.mappings.physical_memory = Some(Mapping::Dynamic);
    config
};

entry_point!(kernel_main, config = &BOOTLOADER_CONFIG);

/// Kernel entry point called by the UEFI bootloader.
fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    // 0. Interrupts stay off until our own GDT/IDT/PIC are installed: the firmware's
    //    interrupt state and vector mapping must not be trusted.
    arch::x86_64::cpu::disable_interrupts();

    // 1. Initialize COM1 Serial port first (before any other output).
    drivers::serial::SERIAL1.lock().init();

    // 2. Identify CPU — pure register read, no memory borrow.
    let cpu_vendor = arch::x86_64::cpu::get_cpu_vendor();

    // 3. Parse Boot Information & Memory Map before taking mutable framebuffer reference.
    //    We extract what we need from boot_info while it is still immutably accessible.
    let mem = {
        let summary = boot::KernelBootInfo::parse(boot_info);
        summary.memory_summary
    };
    let phys_offset = boot_info.physical_memory_offset.into_option();

    // 4. Initialize Framebuffer console (requires mutable borrow of boot_info.framebuffer).
    if let Some(framebuffer_raw) = boot_info.framebuffer.as_mut() {
        let info = framebuffer_raw.info();
        // SAFETY: 'static lifetime is guaranteed by the bootloader — the framebuffer
        // region is mapped for the full kernel lifetime and never reclaimed.
        let buffer: &'static mut [u8] = unsafe {
            core::slice::from_raw_parts_mut(
                framebuffer_raw.buffer_mut().as_mut_ptr(),
                framebuffer_raw.buffer_mut().len(),
            )
        };
        framebuffer::init(buffer, info);
    }

    // 5. Initialize Logger (dual-output: Serial COM1 + Framebuffer).
    logging::init(LevelFilter::Info);

    // 6. Welcome Banner — Framebuffer
    framebuffer::set_color(framebuffer::Color::CYAN);
    crate::fb_println!("FLX KERNEL");
    crate::fb_println!("-------------------------");
    framebuffer::set_color(framebuffer::Color::WHITE);
    crate::fb_println!("Version:      {}", KERNEL_VERSION);
    crate::fb_println!("Architecture: x86_64 ({})", cpu_vendor.as_str());
    crate::fb_println!("Boot:         UEFI");
    crate::fb_println!("Runtime:      Rust no_std");
    crate::fb_println!();

    // 6b. Welcome Banner — Serial
    crate::serial_println!("==================================================");
    crate::serial_println!("FLX KERNEL v{} (Florynx Labs)", KERNEL_VERSION);
    crate::serial_println!("Architecture: x86_64 ({})", cpu_vendor.as_str());
    crate::serial_println!("Boot:         UEFI");
    crate::serial_println!("Runtime:      Rust no_std");
    crate::serial_println!("==================================================");

    // 7. Subsystem Status
    kok!("Boot");
    kok!("CPU ({})", cpu_vendor.as_str());
    kok!(
        "Memory map ({} regions, {} MB usable / {} MB total)",
        mem.region_count,
        mem.usable_bytes / (1024 * 1024),
        mem.total_bytes / (1024 * 1024)
    );
    kok!("Framebuffer");

    // 8. CPU tables: GDT + TSS, then IDT (exceptions) and a breakpoint self-test.
    arch::x86_64::gdt::init();
    kok!("GDT + TSS (IST: double fault, NMI)");
    arch::x86_64::idt::init();
    arch::x86_64::interrupts::trigger_breakpoint();
    kok!("IDT + CPU exceptions (breakpoint self-test)");

    // 9. Physical memory: frame allocator + self-test.
    match phys_offset {
        Some(offset) => {
            // SAFETY: The bootloader maps all physical memory at `offset` (Mapping::Dynamic)
            // and reports only truly free ranges as `Usable`.
            match unsafe { mm::frame::init(&boot_info.memory_regions, offset) } {
                Ok(stats) => {
                    kok!(
                        "Frame allocator ({} frames tracked, {} MB free)",
                        stats.total_frames,
                        stats.free_frames as u64 * mm::frame::FRAME_SIZE / (1024 * 1024)
                    );
                    match mm::frame::self_test() {
                        Ok(()) => kok!("Frame allocator self-test"),
                        Err(e) => kerror!("Frame allocator self-test FAILED: {}", e),
                    }
                }
                Err(e) => kerror!("Frame allocator init failed: {:?}", e),
            }
        }
        None => kerror!("Frame allocator: no physical memory mapping from bootloader"),
    }

    // 10. Hardware interrupts: PIC remap, PIT timer (100 Hz), PS/2 keyboard.
    drivers::pic::init();
    drivers::pit::init();
    drivers::keyboard::init();
    arch::x86_64::cpu::enable_interrupts();

    // Bounded wait: prove the timer IRQ fires without risking a hang if it does not.
    let mut spins: u64 = 0;
    while drivers::pit::ticks() < 10 && spins < 200_000_000 {
        core::hint::spin_loop();
        spins += 1;
    }
    if drivers::pit::ticks() >= 10 {
        kok!(
            "Timer IRQ (PIT {} Hz, {} ticks, uptime {} ms)",
            drivers::pit::TICK_HZ,
            drivers::pit::ticks(),
            drivers::pit::uptime_ms()
        );
    } else {
        kerror!("Timer IRQ: no ticks received");
    }
    kok!("Keyboard IRQ (PS/2, press a key to test)");

    crate::fb_println!();
    framebuffer::set_color(framebuffer::Color::GREEN);
    crate::fb_println!("Kernel initialized.");
    framebuffer::set_color(framebuffer::Color::WHITE);

    crate::serial_println!();
    crate::serial_println!("[OK] Kernel initialized.");
    crate::serial_println!("[INFO] Entering safe kernel idle loop...");

    // 11. Idle loop: sleep until an interrupt, echo typed characters.
    loop {
        while let Some(c) = drivers::keyboard::read_char() {
            crate::serial_print!("{}", c as char);
            crate::fb_print!("{}", c as char);
        }
        arch::x86_64::cpu::halt();
    }
}
