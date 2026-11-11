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
        self.assertIn("arch::activate_address_space(self.root_phys)", source)
        self.assertIn("arch::nx_enabled()", source)
        self.assertNotIn("core::arch::asm!", source)

    def test_arch_facade_selects_target_implementation(self):
        source = (ROOT / "kernel/src/arch/mod.rs").read_text()
        self.assertIn('cfg(target_arch = "x86_64")', source)
        self.assertIn('cfg(target_arch = "aarch64")', source)
        self.assertIn("interface::CpuArchitecture", source)

    def test_aarch64_el1_vectors_cover_all_exception_classes(self):
        source = (ROOT / "kernel/src/arch/aarch64/exceptions.rs").read_text()
        self.assertIn(".balign 2048", source)
        self.assertIn("msr VBAR_EL1", source)
        self.assertIn("ESR_EL1", source)
        self.assertIn("ELR_EL1", source)
        self.assertIn("FAR_EL1", source)
        self.assertIn("__flx_aarch64_irq_entry", source)
        self.assertIn("__flx_aarch64_fiq_entry", source)
        self.assertIn("__flx_aarch64_serror_entry", source)
        self.assertIn("stp x28, x29", source)
        self.assertIn("eret", source)

    def test_aarch64_timer_and_gic_use_real_irq_path(self):
        gic = (ROOT / "kernel/src/arch/aarch64/gic.rs").read_text()
        timer = (ROOT / "kernel/src/arch/aarch64/timer.rs").read_text()
        exceptions = (ROOT / "kernel/src/arch/aarch64/exceptions.rs").read_text()
        self.assertIn("GICR_WAKER", gic)
        # QEMU's GICv3 model completes these writes synchronously.
        # Use DSB + readback rather than polling RWP indefinitely.
        self.assertIn("GICR_CTLR.RWP is RAZ/WI", gic)
        self.assertIn("DistributorConfigurationFailed", gic)
        self.assertIn("CpuInterfaceUnavailable", gic)
        self.assertIn('"dsb sy"', gic)
        self.assertNotIn("wait_for_rwp(", gic)
        self.assertIn("PpiConfigurationFailed", gic)
        self.assertIn("ICC_IAR1_EL1", gic)
        self.assertIn("ICC_EOIR1_EL1", gic)
        self.assertIn("CNTP_TVAL_EL0", timer)
        self.assertIn("CNTFRQ_EL0", timer)
        self.assertIn("__flx_aarch64_irq_dispatch", exceptions)
        self.assertIn("super::timer::handle_interrupt(interrupt_id)", exceptions)

    def test_aarch64_spinlock_masks_only_irq_and_preserves_daif_state(self):
        source = (ROOT / "kernel/src/arch/aarch64/mod.rs").read_text()
        self.assertIn('asm!("msr daifset, #2"', source)
        self.assertIn('asm!("msr daifclr, #2"', source)
        self.assertNotIn('daifset, #0xf', source)
        self.assertNotIn('daifclr, #0xf', source)

if __name__ == "__main__":
    unittest.main()
