#!/usr/bin/env bash
# Collect host-only allocator microbenchmarks. These are not kernel/QEMU timings.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

COMMIT="${FLX_BENCH_COMMIT:-$(git rev-parse HEAD)}"
RUSTC_VERSION="$(rustc --version)"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
OUT_DIR="${FLX_BENCH_OUTPUT_DIR:-benchmarks/results/host}"
mkdir -p "$OUT_DIR"

OUT="$OUT_DIR/${COMMIT}-${STAMP}"
export FLX_BENCH_COMMIT="$COMMIT"
export FLX_BENCH_RUSTC="$RUSTC_VERSION"

cargo run --release --package flx-mm-tests --bin bench_memory \
  > "${OUT}.csv" \
  2> "${OUT}.summary.txt"

printf 'CSV: %s\nSummary/environment: %s\n' "${OUT}.csv" "${OUT}.summary.txt"
