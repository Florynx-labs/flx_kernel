#!/usr/bin/env bash
# Boots the opt-in Ring 3 ELF payload and verifies its SYS_WRITE marker on COM1.
# Usage: tools/ci/check_userspace_smoke.sh [timeout_seconds]
set -uo pipefail

TIMEOUT="${1:-150}"
LOG_RAW="userspace-serial.raw.log"
LOG="userspace-serial.log"
KERNEL_ELF="target/x86_64-unknown-none/debug/flx-kernel"

# Preserve timeout's status without allowing set -e callers to hide the log.
set +e
timeout --signal=TERM --kill-after=5s "${TIMEOUT}" cargo run --package flx-runner -- --userspace-smoke -display none -no-reboot > "${LOG_RAW}" 2>&1
status=$?
set -e
sed -e 's/\\x1b\\[[0-9;]*m//g' -e 's/\\r//g' "${LOG_RAW}" > "${LOG}"

echo "================ userspace serial log ================"
cat "${LOG}"
echo "======================================================="

# The bootloader relocates the kernel by virtual_address_offset. RIP values in
# exception logs are runtime VAs, while addr2line expects linked ELF VAs. Remove
# that relocation before symbol lookup; feeding raw RIPs only produces ?? :0.
if [ -f "${KERNEL_ELF}" ] && command -v addr2line >/dev/null 2>&1; then
  relocation=$(sed -nE 's/.*virtual_address_offset: 0x([0-9a-fA-F]+).*/\\1/p' "${LOG}" | head -n 1)
  addresses=$(grep -oE 'RIP=0x[0-9a-fA-F]+' "${LOG}" | cut -d= -f2 | sort -u || true)
  if [ -n "${addresses}" ]; then
    echo "================ kernel RIP symbols =================="
    if [ -z "${relocation}" ]; then
      echo "WARNING: bootloader relocation offset not found; symbol lookup may be inaccurate."
      relocation=0
    else
      echo "Kernel virtual relocation offset: 0x${relocation}"
    fi
    while IFS= read -r address; do
      [ -n "${address}" ] || continue
      normalized=$(python3 tools/ci/symbolize_rip.py "${address}" "0x${relocation}") || {
        echo "WARNING: could not normalize ${address}; skipping symbol lookup."
        continue
      }
      echo "${address} -> linked VA ${normalized}"
      addr2line -e "${KERNEL_ELF}" -f -C "${normalized}" || true
    done <<< "${addresses}"
    echo "======================================================="
  fi
else
  echo "WARNING: kernel ELF or addr2line unavailable; RIPs were not symbolized."
fi

# QEMU is expected to remain alive after the userspace payload prints its marker.
# The timeout is only a bounded stop mechanism; it is never considered success by itself.
if [ "${status}" -ne 124 ]; then
  python3 tools/ci/annotate.py "${LOG}" "userspace smoke: runner exited before timeout (status ${status})" 7000
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
