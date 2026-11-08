# FLX Kernel — Boot subsystem

## x86_64

The production kernel boot path currently uses the `bootloader_api` UEFI loader on x86_64. CI boots it under QEMU/OVMF and validates the serial log, including memory initialization, interrupts, paging, and the Ring 3 userspace smoke test.

## AArch64 first-stage bring-up

A separate `flx-aarch64-uefi-smoke` image is built for `aarch64-unknown-uefi` and installed at the standard removable-media path `EFI/BOOT/BOOTAA64.EFI`. The first-stage image initializes the QEMU virt PL011 UART at `0x09000000`, emits exactly one `FLX AARCH64 UEFI BOOT: PASS` marker, and waits.

CI runs this image with AArch64 UEFI firmware under `qemu-system-aarch64 -machine virt`. This proves that an AArch64 UEFI application can be built, discovered, loaded, and executed by QEMU firmware. The image also compiles the production AArch64 CPU HAL, installs its 2 KiB-aligned EL1 vector table, and triggers a controlled `BRK #0xF00` exception. The synchronous handler saves/restores all general-purpose registers, records ESR_EL1/ELR_EL1/FAR_EL1, advances ELR_EL1 by one instruction only for the expected BRK, and returns with `eret`. IRQ, FIQ, and SError have distinct vector entries. The QEMU virt GICv3 backend wakes the boot CPU redistributor, enables the Group 1 EL1 physical timer PPI (INTID 30), configures the ICC system-register interface, and acknowledges/EOIs interrupts. The Generic Timer programs periodic `CNTP_TVAL_EL0` ticks from `CNTFRQ_EL0`; IRQ delivery remains masked until the vector table and GIC are ready, then the IRQ handler preserves/restores x0–x30 and rearms the timer. FIQ and SError remain fail-stop during this first stage. This is **not yet the full FLX kernel port**: the existing x86_64 `bootloader_api` entry point, memory initialization, timer/GIC, and scheduler are not wired into this image. Do not describe AArch64 as feature-complete or generally bootable until those stages are integrated and tested.

## Validation levels

- **Cross-compilation:** Rust emits the AArch64 UEFI PE/COFF image.
- **Firmware boot:** QEMU UEFI loads the image from `EFI/BOOT/BOOTAA64.EFI`.
- **Runtime marker:** PL011 serial output confirms entry into the AArch64 image.
- **Full kernel boot:** remains a later milestone requiring memory, exception, interrupt, and kernel subsystem initialization.
