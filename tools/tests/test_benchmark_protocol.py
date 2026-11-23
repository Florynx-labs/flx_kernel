"""Regression checks for reproducible, honestly labeled host benchmarks."""

from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[2]


class BenchmarkProtocolTests(unittest.TestCase):
    def test_harness_preserves_raw_samples_and_distribution_statistics(self):
        source = (ROOT / "tools/mm-tests/src/bin/bench_memory.rs").read_text()
        self.assertIn("const WARMUP_BATCHES: usize = 5;", source)
        self.assertIn("const SAMPLES: usize = 30;", source)
        self.assertIn("benchmark,target,commit,environment,iteration,elapsed_ns,unit,notes", source)
        self.assertIn("median_ns={median}", source)
        self.assertIn("p95_ns={p95}", source)
        self.assertIn("p99_ns={p99}", source)

    def test_host_harness_does_not_claim_kernel_lock_or_qemu_timings(self):
        source = (ROOT / "tools/mm-tests/src/bin/bench_memory.rs").read_text()
        protocol = (ROOT / "docs/performance/benchmark-protocol.md").read_text()
        self.assertIn("std::Mutex", source)
        self.assertIn("NOT kernel/QEMU timings", source)
        self.assertIn("kernel spinlock", protocol)
        self.assertIn("Kernel baseline: not yet measured.", protocol)

    def test_runner_records_revision_and_host_environment(self):
        runner = (ROOT / "tools/benchmarks/run-host-memory.sh").read_text()
        self.assertIn("git rev-parse HEAD", runner)
        self.assertIn("git status --porcelain", runner)
        self.assertIn("rustc -vV", runner)
        self.assertIn("/proc/cpuinfo", runner)
        self.assertIn("benchmarks/results/host", runner)


if __name__ == "__main__":
    unittest.main(verbosity=2)
