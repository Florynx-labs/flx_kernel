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

    def test_riscv_is_not_claimed_as_implemented(self):
        doc = (ROOT / "docs/architecture/multiarch-implementation-status.md").read_text()
        self.assertIn("| RISC-V 64 |", doc)
        self.assertIn("RISC-V remains planned", doc)
        self.assertIn("Do not add fake/no-op CPU primitives", doc)

    def test_performance_baseline_is_not_fabricated(self):
        doc = (ROOT / "docs/performance/benchmark-protocol.md").read_text()
        self.assertIn("**Not yet measured.**", doc)
        self.assertIn("raw samples", doc)
        self.assertIn("median", doc)
        self.assertIn("p95", doc)
        self.assertIn("p99", doc)


if __name__ == "__main__":
    unittest.main()
