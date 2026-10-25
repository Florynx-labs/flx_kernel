//! Logging subsystem for FLX Kernel.
//!
//! Dual-output logger targeting both Serial COM1 (with ANSI escape colors)
//! and Framebuffer graphics console (with 24-bit RGB colors).

use core::fmt;
use log::{Level, LevelFilter, Metadata, Record};
use crate::framebuffer::{self, Color};

/// Static kernel logger instance.
pub static LOGGER: KernelLogger = KernelLogger;

/// Initializes the kernel logging system with the given maximum level.
pub fn init(max_level: LevelFilter) {
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(max_level);
}

pub struct KernelLogger;

impl log::Log for KernelLogger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let (tag, fb_color, serial_color) = match record.level() {
            Level::Error => ("ERROR", Color::RED,    "\x1b[91m"),
            Level::Warn  => ("WARN ", Color::YELLOW, "\x1b[93m"),
            Level::Info  => ("INFO ", Color::CYAN,   "\x1b[96m"),
            Level::Debug => ("DEBUG", Color::PURPLE, "\x1b[95m"),
            Level::Trace => ("TRACE", Color::GRAY,   "\x1b[90m"),
        };

        // 1. Serial COM1 with ANSI colors
        crate::serial_println!("{}[{}]\x1b[0m {}", serial_color, tag, record.args());

        // 2. Framebuffer with RGB colors
        framebuffer::set_color(fb_color);
        crate::fb_print!("[{}] ", tag);
        framebuffer::set_color(Color::WHITE);
        crate::fb_println!("{}", record.args());
    }

    fn flush(&self) {}
}

/// Internal helper for structured status logging.
pub fn log_status(tag: &str, fb_color: Color, serial_ansi: &str, args: fmt::Arguments) {
    // Serial
    crate::serial_println!("{}[{}]\x1b[0m {}", serial_ansi, tag, args);

    // Framebuffer
    framebuffer::set_color(fb_color);
    crate::fb_print!("[{}] ", tag);
    framebuffer::set_color(Color::WHITE);
    crate::fb_println!("{}", args);
}

/// Logs an "[OK]" status message.
#[macro_export]
macro_rules! kok {
    ($($arg:tt)*) => {{
        $crate::logging::log_status("OK   ", $crate::framebuffer::Color::GREEN, "\x1b[92m", core::format_args!($($arg)*));
    }};
}

/// Logs an "[INFO]" message.
#[macro_export]
macro_rules! kinfo {
    ($($arg:tt)*) => {{
        $crate::logging::log_status("INFO ", $crate::framebuffer::Color::CYAN, "\x1b[96m", core::format_args!($($arg)*));
    }};
}

/// Logs a "[WARN]" warning message.
#[macro_export]
macro_rules! kwarn {
    ($($arg:tt)*) => {{
        $crate::logging::log_status("WARN ", $crate::framebuffer::Color::YELLOW, "\x1b[93m", core::format_args!($($arg)*));
    }};
}

/// Logs an "[ERROR]" error message.
#[macro_export]
macro_rules! kerror {
    ($($arg:tt)*) => {{
        $crate::logging::log_status("ERROR", $crate::framebuffer::Color::RED, "\x1b[91m", core::format_args!($($arg)*));
    }};
}

/// Logs a "[DEBUG]" debug message.
#[macro_export]
macro_rules! kdebug {
    ($($arg:tt)*) => {{
        $crate::logging::log_status("DEBUG", $crate::framebuffer::Color::PURPLE, "\x1b[95m", core::format_args!($($arg)*));
    }};
}
