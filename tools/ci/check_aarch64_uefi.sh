#!/usr/bin/env bash
# Boots the AArch64 UEFI first-stage image under QEMU virt.
set -uo pipefail

TIMEOUT="${1:-30}"
CODE_FD="${AAVMF_CODE:-}"
VARS_FD="${AAVMF_VARS:-}"

if [[ -z "$CODE_FD" ]]; then
  for candidate in /usr/share/AAVMF/AAVMF_CODE.fd /usr/share/qemu-efi-aarch64/QEMU_EFI.fd; do
    if [[ -f "$candidate" ]]; then CODE_FD="$candidate"; break; fi
  done
fi
if [[ -z "$VARS_FD" ]]; then
  for candidate in /usr/share/AAVMF/AAVMF_VARS.fd /usr/share/qemu-efi-aarch64/QEMU_VARS.fd; do
    if [[ -f "$candidate" ]]; then VARS_FD="$candidate"; break; fi
  done
fi
if [[ -z "$CODE_FD" || ! -f "$CODE_FD" ]]; then
  echo "::error::AArch64 UEFI firmware code image not found"
  exit 1
fi

VARS_COPY="$(mktemp)"
LOG="aarch64-uefi-serial.log"
ESP="$(mktemp -d)"
cleanup() { rm -f "$VARS_COPY"; rm -rf "$ESP"; }
trap cleanup EXIT

if [[ -z "$VARS_FD" || ! -f "$VARS_FD" ]]; then
  echo "::error::AArch64 UEFI firmware variables image not found"
  exit 1
fi
cp "$VARS_FD" "$VARS_COPY"

mkdir -p "$ESP/EFI/BOOT"
cp target/aarch64-unknown-uefi/release/BOOTAA64.efi "$ESP/EFI/BOOT/BOOTAA64.EFI"

set +e
timeout --signal=TERM --kill-after=5s "$TIMEOUT" qemu-system-aarch64 \
  -machine virt,gic-version=3 -cpu cortex-a57 -m 512M -nographic -no-reboot -net none \
  -drive if=pflash,format=raw,readonly=on,file="$CODE_FD" \
  -drive if=pflash,format=raw,file="$VARS_COPY" \
  -drive format=raw,file="fat:rw:$ESP" \
  -serial stdio -monitor none > "$LOG" 2>&1
status=$?
set -e

cat "$LOG"
if [[ "$status" -ne 124 ]]; then
  echo "::error::QEMU AArch64 UEFI exited unexpectedly (status $status)"
  exit 1
fi
if [[ "$(grep -cF 'FLX AARCH64 UEFI BOOT: PASS' "$LOG")" -ne 1 ]]; then
  echo "::error::AArch64 UEFI boot marker missing or duplicated"
  exit 1
fi
if [[ "$(grep -cF 'FLX AARCH64 EXCEPTION: PASS' "$LOG")" -ne 1 ]]; then
  echo "::error::AArch64 controlled exception test did not pass exactly once"
  exit 1
fi
if [[ "$(grep -cF 'FLX AARCH64 TIMER: PASS' "$LOG")" -ne 1 ]]; then
  echo "::error::AArch64 periodic timer test did not pass exactly once"
  exit 1
fi
if grep -qF 'FLX AARCH64 TIMER: FAIL' "$LOG" || grep -qF 'FLX AARCH64 TIMER: INIT FAIL' "$LOG"; then
  echo "::error::AArch64 timer test failed"
  exit 1
fi
if grep -qF 'FLX AARCH64 EXCEPTION: FAIL' "$LOG"; then
  echo "::error::AArch64 controlled exception test failed"
  exit 1
fi
if grep -qF 'FLX AARCH64 UEFI BOOT: PANIC' "$LOG"; then
  echo "::error::AArch64 UEFI image panicked"
  exit 1
fi

echo "FLX AArch64 UEFI boot validation: PASSED"
