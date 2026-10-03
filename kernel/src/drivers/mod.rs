//! Drivers subsystem for FLX Kernel.
//!
//! Provides hardware abstraction drivers for platform devices, starting with
//! standard x86 serial communications (16550 UART) for debugging and logging.

pub mod serial;
