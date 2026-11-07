"""Tests for the CI runtime-to-linked-address normalization helper."""

from pathlib import Path
import subprocess
import sys
import unittest

ROOT = Path(__file__).resolve().parents[2]
HELPER = ROOT / "tools/ci/symbolize_rip.py"


class RipSymbolizationTests(unittest.TestCase):
    def normalize(self, rip: str, offset: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [sys.executable, str(HELPER), rip, offset],
            check=False,
            capture_output=True,
            text=True,
        )

    def test_removes_bootloader_virtual_relocation(self):
        result = self.normalize("0x1000000c92f", "0x10000000000")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), "0xc92f")

    def test_preserves_already_linked_address_with_zero_offset(self):
        result = self.normalize("0xc92f", "0")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), "0xc92f")

    def test_rejects_rip_below_relocation_base(self):
        result = self.normalize("0x1000", "0x10000000000")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("below the kernel relocation base", result.stderr)


if __name__ == "__main__":
    unittest.main(verbosity=2)
