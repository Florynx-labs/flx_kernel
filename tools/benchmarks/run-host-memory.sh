#!/usr/bin/env bash
# Collect host-only allocator microbenchmarks. These are not kernel/QEMU timings.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

COMMIT="${FLX_BENCH_COMMIT:-$(git rev-parse HEAD)}"
RUSTC_VERSION="$(rustc --version)"
RUSTC_HOST="$(rustc -vV | sed -n 's/^host: //p')"
WORKTREE_STATUS="$(git status --porcelain)"
WORKTREE_CLEAN="true"
if [[ -n "$WORKTREE_STATUS" ]]; then WORKTREE_CLEAN="false"; fi
HOST_KERNEL="$(uname -srmo)"
HOST_CPU="$(awk -F: '/model name/ {gsub(/^[ \t]+/, "", $2); print $2; exit}' /proc/cpuinfo 2>/dev/null || true)"
HOST_CPU="${HOST_CPU:-unknown}"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
OUT_DIR="${FLX_BENCH_OUTPUT_DIR:-benchmarks/results/host}"
mkdir -p "$OUT_DIR"

OUT="$OUT_DIR/${COMMIT}-${STAMP}"
export FLX_BENCH_COMMIT="$COMMIT"
export FLX_BENCH_RUSTC="$RUSTC_VERSION"
export FLX_BENCH_RUSTC_HOST="$RUSTC_HOST"
export FLX_BENCH_CLEAN="$WORKTREE_CLEAN"
WORKTREE_STATUS_ONE_LINE="$(printf '%s' "$WORKTREE_STATUS" | tr '\n' ';')"
export FLX_BENCH_WORKTREE_STATUS="$WORKTREE_STATUS_ONE_LINE"
export FLX_BENCH_HOST_KERNEL="$HOST_KERNEL"
export FLX_BENCH_HOST_CPU="$HOST_CPU"

cargo run --release --package flx-mm-tests --bin bench_memory \
  > "${OUT}.csv" \
  2> "${OUT}.summary.txt"

printf 'CSV: %s\nSummary/environment: %s\n' "${OUT}.csv" "${OUT}.summary.txt"
\n'/; }"
export FLX_BENCH_HOST_KERNEL="$HOST_KERNEL"
export FLX_BENCH_HOST_CPU="$HOST_CPU"

cargo run --release --package flx-mm-tests --bin bench_memory \
  > "${OUT}.csv" \
  2> "${OUT}.summary.txt"

printf 'CSV: %s\nSummary/environment: %s\n' "${OUT}.csv" "${OUT}.summary.txt"
