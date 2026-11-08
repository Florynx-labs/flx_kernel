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

    def test_paging_delegates_privileged_register_operations(self):
        source = (ROOT / "kernel/src/mm/paging.rs").read_text()
        self.assertIn("arch::current_address_space_root()", source)
        self.assertIn("arch::invalidate_tlb_page(address)", source)
        self.assertNotIn("core::arch::asm!", source)

    def test_arch_facade_selects_target_implementation(self):
        source = (ROOT / "kernel/src/arch/mod.rs").read_text()
        self.assertIn('cfg(target_arch = "x86_64")', source)
        self.assertIn('cfg(target_arch = "aarch64")', source)
        self.assertIn("interface::CpuArchitecture", source)

    def test_aarch64_spinlock_masks_only_irq_and_preserves_daif_state(self):
        source = (ROOT / "kernel/src/arch/aarch64/mod.rs").read_text()
        self.assertIn('asm!("msr daifset, #2"', source)
        self.assertIn('asm!("msr daifclr, #2"', source)
        self.assertNotIn('daifset, #0xf', source)
        self.assertNotIn('daifclr, #0xf', source)

if __name__ == "__main__":
    unittest.main()
