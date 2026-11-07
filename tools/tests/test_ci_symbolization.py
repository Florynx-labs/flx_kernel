"""Tests for the CI runtime-to-linked-address normalization helper."""

from pathlib import Path
import re
import subprocess
import sys
import unittest

ROOT = Path(__file__).resolve().parents[2]
HELPER = ROOT / "tools/ci/symbolize_rip.py"
SMOKE = (ROOT / "tools/ci/check_userspace_smoke.sh").read_text(encoding="utf-8")
BOOT = (ROOT / "tools/ci/check_boot.sh").read_text(encoding="utf-8")


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

    def test_smoke_script_uses_relocation_helper_and_requires_real_marker(self):
        self.assertIn("tools/ci/symbolize_rip.py", SMOKE)
        self.assertIn('grep -qF -- "FLX USERSPACE: PASS"', SMOKE)
        self.assertIn('[ "${status}" -ne 124 ]', SMOKE)
        self.assertIn("timeout is only a bounded stop mechanism", SMOKE)

    def test_boot_script_checks_heap_growth_and_paging(self):
        self.assertIn('require "[OK   ] Heap growth self-test ("', BOOT)
        self.assertIn('require "[OK   ] Paging (active CR3"', BOOT)

    def test_smoke_script_extracts_the_relocation_offset_from_boot_log(self):
        match = re.search(r"relocation=\$\(sed -nE '([^']+)'", SMOKE)
        self.assertIsNotNone(match, "relocation sed expression missing")
        result = subprocess.run(
            ["sed", "-nE", match.group(1)],
            input="INFO : virtual_address_offset: 0x10000000000\n",
            capture_output=True,
            text=True,
            check=True,
        )
        self.assertEqual(result.stdout.strip(), "10000000000")


if __name__ == "__main__":
    unittest.main(verbosity=2)
