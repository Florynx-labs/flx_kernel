//! PS/2 keyboard driver (IRQ 1, scancode set 1, US QWERTY layout).
//!
//! Decoded characters are queued in a fixed-size ring buffer and can be read with
//! [`read_char`]. Dead keys, AltGr, function keys and non-US layouts are NOT IMPLEMENTED.

use crate::arch::x86_64::interrupts::{register_handler, InterruptFrame, IRQ_BASE};
use crate::arch::x86_64::port::Port;
use crate::drivers::pic;
use crate::sync::Spinlock;

const DATA_PORT: u16 = 0x60;
const BUFFER_SIZE: usize = 128;

/// Scancode set 1 -> ASCII (unshifted), indices 0x00..=0x39. `0` = no printable mapping.
const MAP_NORMAL: [u8; 0x3A] = [
    0, 27, b'1', b'2', b'3', b'4', b'5', b'6', b'7', b'8', b'9', b'0', b'-', b'=', 8, b'\t', //
    b'q', b'w', b'e', b'r', b't', b'y', b'u', b'i', b'o', b'p', b'[', b']', b'\n', 0, b'a', b's', //
    b'd', b'f', b'g', b'h', b'j', b'k', b'l', b';', b'\'', b'`', 0, b'\\', b'z', b'x', b'c', b'v', //
    b'b', b'n', b'm', b',', b'.', b'/', 0, b'*', 0, b' ',
];

/// Scancode set 1 -> ASCII with Shift held.
const MAP_SHIFT: [u8; 0x3A] = [
    0, 27, b'!', b'@', b'#', b'$', b'%', b'^', b'&', b'*', b'(', b')', b'_', b'+', 8, b'\t', //
    b'Q', b'W', b'E', b'R', b'T', b'Y', b'U', b'I', b'O', b'P', b'{', b'}', b'\n', 0, b'A', b'S', //
    b'D', b'F', b'G', b'H', b'J', b'K', b'L', b':', b'"', b'~', 0, b'|', b'Z', b'X', b'C', b'V', //
    b'B', b'N', b'M', b'<', b'>', b'?', 0, b'*', 0, b' ',
];

struct KeyboardState {
    shift: bool,
    caps_lock: bool,
    extended: bool,
    buffer: [u8; BUFFER_SIZE],
    head: usize,
    tail: usize,
}

impl KeyboardState {
    const fn new() -> Self {
        Self {
            shift: false,
            caps_lock: false,
            extended: false,
            buffer: [0; BUFFER_SIZE],
            head: 0,
            tail: 0,
        }
    }

    fn push(&mut self, c: u8) {
        let next = (self.head + 1) % BUFFER_SIZE;
        if next != self.tail {
            self.buffer[self.head] = c;
            self.head = next;
        } // else: buffer full, drop the key.
    }

    fn pop(&mut self) -> Option<u8> {
        if self.head == self.tail {
            return None;
        }
        let c = self.buffer[self.tail];
        self.tail = (self.tail + 1) % BUFFER_SIZE;
        Some(c)
    }

    /// Feeds one raw scancode; returns the decoded ASCII character, if any.
    fn feed(&mut self, scancode: u8) -> Option<u8> {
        if scancode == 0xE0 {
            self.extended = true;
            return None;
        }
        let extended = core::mem::replace(&mut self.extended, false);
        let released = scancode & 0x80 != 0;
        let code = scancode & 0x7F;

        match code {
            0x2A | 0x36 if !extended => {
                self.shift = !released;
                return None;
            }
            0x3A if !extended && !released => {
                self.caps_lock = !self.caps_lock;
                return None;
            }
            _ => {}
        }

        if released || extended || code as usize >= MAP_NORMAL.len() {
            return None;
        }

        let base = MAP_NORMAL[code as usize];
        let c = if base.is_ascii_lowercase() {
            // Caps Lock inverts the effect of Shift on letters only.
            if self.shift ^ self.caps_lock {
                MAP_SHIFT[code as usize]
            } else {
                base
            }
        } else if self.shift {
            MAP_SHIFT[code as usize]
        } else {
            base
        };

        if c == 0 {
            None
        } else {
            Some(c)
        }
    }
}

static KEYBOARD: Spinlock<KeyboardState> = Spinlock::new(KeyboardState::new());

fn on_key(_frame: &mut InterruptFrame) {
    let port = Port::<u8>::new(DATA_PORT);
    // SAFETY: Reading the PS/2 controller output buffer; required to re-arm the IRQ.
    let scancode = unsafe { port.read() };

    let mut kb = KEYBOARD.lock();
    if let Some(c) = kb.feed(scancode) {
        kb.push(c);
    }
}

/// Registers the IRQ 1 handler and unmasks the line.
pub fn init() {
    register_handler(IRQ_BASE + 1, on_key);
    pic::unmask(1);
}

/// Returns the next decoded character from the keyboard buffer, if any.
pub fn read_char() -> Option<u8> {
    KEYBOARD.lock().pop()
}
