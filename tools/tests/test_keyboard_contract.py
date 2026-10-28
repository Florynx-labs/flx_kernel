"""Source-contract tests for the PS/2 keyboard decoder and IRQ wiring."""
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
KEYBOARD = (ROOT / "kernel/src/drivers/keyboard.rs").read_text(encoding="utf-8")


class KeyboardContractTests(unittest.TestCase):
    def test_ring_buffer_is_bounded_and_drops_when_full(self):
        self.assertIn("const BUFFER_SIZE: usize = 128;", KEYBOARD)
        self.assertIn("let next = (self.head + 1) % BUFFER_SIZE;", KEYBOARD)
        self.assertIn("if next != self.tail", KEYBOARD)
        self.assertIn("self.tail = (self.tail + 1) % BUFFER_SIZE;", KEYBOARD)

    def test_shift_and_caps_lock_are_tracked_independently(self):
        self.assertIn("shift: bool,", KEYBOARD)
        self.assertIn("caps_lock: bool,", KEYBOARD)
        self.assertIn("self.shift = !released;", KEYBOARD)
        self.assertIn("self.caps_lock = !self.caps_lock;", KEYBOARD)
        # Letters use XOR so Shift reverses Caps Lock; punctuation uses Shift only.
        self.assertIn("self.shift ^ self.caps_lock", KEYBOARD)
        self.assertIn("} else if self.shift {", KEYBOARD)

    def test_key_releases_and_extended_keys_are_not_printed(self):
        self.assertIn("if scancode == 0xE0", KEYBOARD)
        self.assertIn("let released = scancode & 0x80 != 0;", KEYBOARD)
        self.assertIn("if released || extended || code as usize >= MAP_NORMAL.len()", KEYBOARD)

    def test_keyboard_irq_is_registered_and_unmasked(self):
        self.assertIn("register_handler(IRQ_BASE + 1, on_key);", KEYBOARD)
        self.assertIn("pic::unmask(1);", KEYBOARD)
        self.assertIn("let scancode = unsafe { port.read() };", KEYBOARD)

    def test_userspace_reads_decoded_characters_through_api(self):
        self.assertIn("pub fn read_char() -> Option<u8>", KEYBOARD)
        self.assertIn("KEYBOARD.lock().pop()", KEYBOARD)


if __name__ == "__main__":
    unittest.main(verbosity=2)
