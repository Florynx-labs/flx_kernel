//! Drivers subsystem for FLX Kernel.
//!
//! Provides hardware abstraction drivers for platform devices: serial (16550 UART),
//! legacy PIC, PIT system timer and PS/2 keyboard.

pub mod keyboard;
pub mod pic;
pub mod pit;
pub mod serial;
