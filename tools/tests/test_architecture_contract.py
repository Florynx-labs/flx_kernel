import pathlib
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]

class ArchitectureBoundaryTests(unittest.TestCase):
    def test_shared_spinlock_uses_architecture_facade(self):
        source = (ROOT / "kernel/src/sync/spinlock.rs").read_text()
        self.assertIn("crate::arch", source)
        self.assertIn("arch::disable_local_interrupts()", source)
        self.assertIn("arch::enable_local_interrupts()", source)
        self.assertNotIn("arch::x86_64", source)

    def test_arch_facade_selects_target_implementation(self):
        source = (ROOT / "kernel/src/arch/mod.rs").read_text()
        self.assertIn('cfg(target_arch = "x86_64")', source)
        self.assertIn('cfg(target_arch = "aarch64")', source)
        self.assertIn("interface::CpuArchitecture", source)

if __name__ == "__main__":
    unittest.main()
