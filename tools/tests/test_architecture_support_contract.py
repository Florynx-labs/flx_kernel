"""Regression checks for architecture support claims in repository documentation."""

from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[2]


class ArchitectureSupportContractTests(unittest.TestCase):
    def test_aarch64_is_not_mislabeled_as_full_kernel_boot(self):
        doc = (ROOT / "docs/architecture/multiarch-implementation-status.md").read_text()
        self.assertIn("Full FLX kernel entry", doc)
        self.assertIn("Blocked on a target-neutral boot handoff", doc)
        self.assertIn("does **not** call the production kernel entry point", doc)

    def test_riscv_is_compile_only_not_boot_ready(self):
        doc = (ROOT / "docs/architecture/multiarch-implementation-status.md").read_text()
        self.assertIn("| RISC-V 64 |", doc)
        self.assertIn("RISC-V compile-check only", doc)
        self.assertIn("Do not add fake/no-op CPU primitives", doc)

    def test_common_entry_uses_the_cpu_hal_for_interrupts_and_halt(self):
        entry = (ROOT / "kernel/src/main.rs").read_text()
        self.assertIn("arch::disable_local_interrupts()", entry)
        self.assertIn("arch::enable_local_interrupts()", entry)
        self.assertIn("arch::halt_forever()", entry)
        self.assertIn("arch::initialize_cpu_tables()", entry)
        self.assertNotIn("arch::x86_64::cpu::halt()", entry)
        self.assertNotIn("arch::x86_64::gdt::", entry)
        self.assertNotIn("arch::x86_64::idt::", entry)

    def test_aarch64_vector_setup_reports_distinct_failure_reasons(self):
        exceptions = (ROOT / "kernel/src/arch/aarch64/exceptions.rs").read_text()
        self.assertIn("WrongExceptionLevel(u8)", exceptions)
        self.assertIn("MisalignedVectorTable { address: u64 }", exceptions)
        self.assertNotIn("pub unsafe fn install_el1_vectors() -> Result<(), u8>", exceptions)

    def test_performance_baseline_is_not_fabricated(self):
        doc = (ROOT / "docs/performance/benchmark-protocol.md").read_text()
        self.assertIn("**Kernel baseline: not yet measured.**", doc)
        self.assertIn("raw samples", doc)
        self.assertIn("median", doc)
        self.assertIn("p95", doc)
        self.assertIn("p99", doc)


if __name__ == "__main__":
    unittest.main()
