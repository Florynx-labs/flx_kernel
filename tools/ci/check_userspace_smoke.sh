#!/usr/bin/env bash
# Boots the opt-in Ring 3 ELF payload and verifies its SYS_WRITE marker on COM1.
# Usage: tools/ci/check_userspace_smoke.sh [timeout_seconds]
set -uo pipefail

TIMEOUT="${1:-90}"
LOG_RAW="userspace-serial.raw.log"
LOG="userspace-serial.log"

timeout "${TIMEOUT}" cargo run --package flx-runner -- --userspace-smoke -display none -no-reboot > "${LOG_RAW}" 2>&1
status=$?
sed -e 's/\x1b\[[0-9;]*m//g' -e 's/\r//g' "${LOG_RAW}" > "${LOG}"

echo "================ userspace serial log ================"
cat "${LOG}"
echo "======================================================="

# Resolve guest instruction pointers against the exact ELF built by the runner. This turns
# a raw page-fault RIP into a source file/line in CI logs without requiring a debugger.
KERNEL_ELF="target/x86_64-unknown-none/debug/flx-kernel"
ADDR2LINE="$(command -v addr2line || true)"
if [ -z "${ADDR2LINE}" ]; then
  HOST_TRIPLE="$(rustc -vV | sed -n 's/^host: //p')"
  LLVM_BIN="$(rustc --print sysroot)/lib/rustlib/${HOST_TRIPLE}/bin"
  if [ -x "${LLVM_BIN}/llvm-addr2line" ]; then
    ADDR2LINE="${LLVM_BIN}/llvm-addr2line"
  fi
fi
if [ -x "${KERNEL_ELF}" ] && [ -n "${ADDR2LINE}" ]; then
  while IFS= read -r rip; do
    address="${rip#RIP=}"
    echo "[addr2line] ${address}"
    "${ADDR2LINE}" -f -C -e "${KERNEL_ELF}" "${address}" || true
  done < <(grep -oE 'RIP=0x[0-9a-fA-F]+' "${LOG}" | sort -u)
else
  echo "[addr2line] unavailable (kernel ELF or addr2line not found)"
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
