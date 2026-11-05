#!/usr/bin/env bash
# Boots the opt-in Ring 3 ELF payload and verifies its SYS_WRITE marker on COM1.
# Usage: tools/ci/check_userspace_smoke.sh [timeout_seconds]
set -uo pipefail

TIMEOUT="${1:-90}"
LOG_RAW="userspace-serial.raw.log"
LOG="userspace-serial.log"
KERNEL_ELF="target/x86_64-unknown-none/debug/flx-kernel"

timeout "${TIMEOUT}" cargo run --package flx-runner -- --userspace-smoke -display none -no-reboot > "${LOG_RAW}" 2>&1
status=$?
sed -e 's/\\x1b\\[[0-9;]*m//g' -e 's/\\r//g' "${LOG_RAW}" > "${LOG}"

echo "================ userspace serial log ================"
cat "${LOG}"
echo "======================================================="

# Keep debug symbols in the CI build and translate every reported kernel RIP into
# a Rust function/source location. This makes nested faults actionable instead of
# requiring a maintainer to reproduce the exact runner image locally.
if [ -f "${KERNEL_ELF}" ] && command -v addr2line >/dev/null 2>&1; then
  addresses=$(grep -oE 'RIP=0x[0-9a-fA-F]+' "${LOG}" | cut -d= -f2 | sort -u || true)
  if [ -n "${addresses}" ]; then
    echo "================ kernel RIP symbols =================="
    while IFS= read -r address; do
      [ -n "${address}" ] || continue
      echo "${address}"
      addr2line -e "${KERNEL_ELF}" -f -C "${address}" || true
    done <<< "${addresses}"
    echo "======================================================="
  fi
fi

if [ "${status}" -ne 124 ]; then
  python3 tools/ci/annotate.py "${LOG}" "userspace smoke: runner exited early (status ${status})" 7000
  exit 1
fi

if ! grep -qF -- "FLX USERSPACE: PASS" "${LOG}"; then
  python3 tools/ci/annotate.py "${LOG}" "Ring 3 ELF did not emit the expected SYS_WRITE marker" 7000
  exit 1
fi
if grep -qF -- "FLX KERNEL PANIC" "${LOG}" || grep -qF -- "[ERROR]" "${LOG}"; then
  python3 tools/ci/annotate.py "${LOG}" "userspace smoke encountered a kernel error" 7000
  exit 1
fi

python3 tools/ci/annotate.py "${LOG}" "Ring 3 ELF + SYS_WRITE smoke validation PASSED" 7000 notice
echo "FLX Kernel Ring 3 userspace smoke validation: PASSED"
