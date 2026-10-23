#!/usr/bin/env bash
# Boots FLX Kernel headless under QEMU/OVMF and validates the serial log.
# Usage: tools/ci/check_boot.sh [timeout_seconds]
set -uo pipefail

TIMEOUT="${1:-90}"
LOG_RAW="boot-serial.raw.log"
LOG="boot-serial.log"

# The runner builds the kernel, creates the UEFI image and launches QEMU (serial on stdout).
# -no-reboot: a triple fault makes QEMU exit instead of looping forever.
timeout "${TIMEOUT}" cargo run --package flx-runner -- -display none -no-reboot > "${LOG_RAW}" 2>&1
status=$?

# Strip ANSI color codes and carriage returns for stable matching.
sed -e 's/\x1b\[[0-9;]*m//g' -e 's/\r//g' "${LOG_RAW}" > "${LOG}"

echo "================ serial log ================"
cat "${LOG}"
echo "============================================"

# `timeout` kills QEMU after the idle loop is reached: 124 is the expected exit status.
if [ "${status}" -ne 124 ]; then
  echo "::error::QEMU/runner exited early with status ${status} (expected to run until timeout)"
  exit 1
fi

fail=0
require() {
  if ! grep -qF -- "$1" "${LOG}"; then
    echo "::error::missing expected boot line: $1"
    fail=1
  fi
}
forbid() {
  if grep -qF -- "$1" "${LOG}"; then
    echo "::error::unexpected boot line: $1"
    fail=1
  fi
}

require "[OK   ] Boot"
require "[OK   ] Memory map"
require "[OK   ] GDT + TSS"
require "[OK   ] IDT + CPU exceptions"
require "[OK   ] Frame allocator ("
require "[OK   ] Frame allocator self-test"
require "[OK   ] Timer IRQ"
require "[OK   ] Keyboard IRQ"
require "[OK] Kernel initialized."

forbid "FLX KERNEL PANIC"
forbid "[ERROR]"
forbid "FAILED"

if [ "${fail}" -ne 0 ]; then
  exit 1
fi
echo "FLX Kernel boot validation: PASSED"
