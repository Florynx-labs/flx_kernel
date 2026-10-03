//! Standard 16550 UART Serial Driver.
//!
//! Provides bare-metal serial communication over standard PC x86 COM ports (COM1 = 0x3F8).
//! Designed for low-latency kernel logging and early boot console output.

use core::fmt;
use crate::arch::x86_64::port::Port;
use crate::sync::Spinlock;

/// Standard COM1 base I/O port address.
pub const COM1_BASE: u16 = 0x3F8;

/// A 16550 UART serial port controller.
pub struct SerialPort {
    data: Port<u8>,
    int_en: Port<u8>,
    fifo_ctrl: Port<u8>,
    line_ctrl: Port<u8>,
    modem_ctrl: Port<u8>,
    line_status: Port<u8>,
    initialized: bool,
}

impl SerialPort {
    /// Creates a new serial port interface at the given base I/O port.
    pub const fn new(base: u16) -> Self {
        Self {
            data: Port::new(base),
            int_en: Port::new(base + 1),
            fifo_ctrl: Port::new(base + 2),
            line_ctrl: Port::new(base + 3),
            modem_ctrl: Port::new(base + 4),
            line_status: Port::new(base + 5),
            initialized: false,
        }
    }

    /// Initializes the serial port to 115200 baud, 8N1 (8 data bits, no parity, 1 stop bit).
    pub fn init(&mut self) {
        // SAFETY: Direct port I/O to standard 16550 UART registers.
        // Invariant: The base port belongs to standard PC COM hardware.
        unsafe {
            // Disable all interrupts
            self.int_en.write(0x00);

            // Enable DLAB (set baud rate divisor)
            self.line_ctrl.write(0x80);

            // Set divisor to 1 (115200 baud)
            self.data.write(0x01);
            self.int_en.write(0x00);

            // 8 bits, no parity, one stop bit
            self.line_ctrl.write(0x03);

            // Enable FIFO, clear TX/RX queues, with 14-byte threshold
            self.fifo_ctrl.write(0xC7);

            // IRQs enabled, RTS/DSR set
            self.modem_ctrl.write(0x0B);
        }

        self.initialized = true;
    }

    /// Checks if the transmit buffer is empty and ready for a new byte.
    fn is_transmit_empty(&mut self) -> bool {
        // SAFETY: Reading Line Status Register is safe on standard UART.
        unsafe { self.line_status.read() & 0x20 != 0 }
    }

    /// Sends a single raw byte over the serial port.
    pub fn send(&mut self, data: u8) {
        if !self.initialized {
            self.init();
        }

        // Wait until transmit buffer is empty
        while !self.is_transmit_empty() {
            core::hint::spin_loop();
        }

        // SAFETY: Writing data to Data Register when transmit buffer is empty.
        unsafe {
            self.data.write(data);
        }
    }

    /// Sends a string slice over the serial port, converting '\n' to "\r\n".
    pub fn send_str(&mut self, s: &str) {
        for byte in s.bytes() {
            if byte == b'\n' {
                self.send(b'\r');
            }
            self.send(byte);
        }
    }
}

impl fmt::Write for SerialPort {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.send_str(s);
        Ok(())
    }
}

/// Global COM1 serial port protected by a spinlock.
pub static SERIAL1: Spinlock<SerialPort> = Spinlock::new(SerialPort::new(COM1_BASE));

/// Prints formatted text to the global COM1 serial port.
#[macro_export]
macro_rules! serial_print {
    ($($arg:tt)*) => {
        $crate::drivers::serial::_print(core::format_args!($($arg)*))
    };
}

/// Prints formatted text followed by a newline to the global COM1 serial port.
#[macro_export]
macro_rules! serial_println {
    () => ($crate::serial_print!("\n"));
    ($($arg:tt)*) => ($crate::serial_print!("{}\n", core::format_args!($($arg)*)));
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    let mut port = SERIAL1.lock();
    let _ = port.write_fmt(args);
}
