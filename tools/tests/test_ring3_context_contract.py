"""Regression contracts for the first isolated Ring 3 process context."""

from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
PROCESS = (ROOT / "kernel/src/mm/process.rs").read_text(encoding="utf-8")
LOGGING = (ROOT / "kernel/src/logging.rs").read_text(encoding="utf-8")
PANIC = (ROOT / "kernel/src/panic.rs").read_text(encoding="utf-8")


class Ring3ContextContractTests(unittest.TestCase):
    def test_active_process_context_is_owned_by_kernel_static_storage(self):
        self.assertIn("static mut CURRENT_ADDRESS_SPACE_STORAGE: MaybeUninit<AddressSpace>", PROCESS)
        self.assertIn("let process = ManuallyDrop::new(self);", PROCESS)
        self.assertIn("storage.write(address_space)", PROCESS)
        self.assertIn("CURRENT_ADDRESS_SPACE.store(storage, Ordering::Release)", PROCESS)
        self.assertNotIn("addr_of!(self.address_space)", PROCESS)

    def test_user_transition_enables_serial_only_diagnostics(self):
        self.assertIn("crate::logging::set_serial_only();", PROCESS)
        self.assertIn("pub fn set_serial_only()", LOGGING)
        self.assertIn("if serial_only() { return; }", LOGGING)
        self.assertIn("if !serial_only {", PANIC)


if __name__ == "__main__":
    unittest.main(verbosity=2)
