"""Regression checks for the experimental RISC-V support boundary."""

from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[2]


class Riscv64SupportContractTests(unittest.TestCase):
    def test_riscv_primitives_are_real_s_mode_and_sbi_operations(self):
        source = (ROOT / "kernel/src/arch/riscv64/mod.rs").read_text()
        self.assertIn('"csrr {status}, sstatus"', source)
        self.assertIn('"csrc sstatus, {mask}"', source)
        self.assertIn('"csrs sstatus, {mask}"', source)
        self.assertIn('"csrw stvec, {entry}"', source)
        self.assertIn('"ecall"', source)
        self.assertIn("SBI_EXT_TIME", source)
        self.assertIn("SBI_TIME_SET_TIMER", source)

    def test_riscv_is_compile_only_not_a_kernel_boot_claim(self):
        docs = (ROOT / "docs/architecture/multiarch-implementation-status.md").read_text()
        self.assertIn("RISC-V compile-check only", docs)
        self.assertIn("no RISC-V full-kernel boot", docs)
        self.assertIn("PLIC and trap dispatch remain unimplemented", docs)
        self.assertNotIn("RISC-V boot: implemented", docs)

    def test_riscv_module_is_not_misrepresented_as_a_complete_cpu_backend(self):
        source = (ROOT / "kernel/src/arch/riscv64/mod.rs").read_text()
        self.assertNotIn("impl crate::arch::interface::CpuArchitecture", source)
        self.assertIn("not wired into the production kernel entry", source)


if __name__ == "__main__":
    unittest.main(verbosity=2)
