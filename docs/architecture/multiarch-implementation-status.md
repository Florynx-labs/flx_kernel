# FLX Kernel multi-architecture implementation status

This document is the issue-closure contract for the multi-architecture work. A target is not
supported because its Rust primitives compile or because a separate firmware test image boots.

## Status definitions

- **Implemented**: code is part of the real kernel path and has automated regression coverage.
- **Experimental**: isolated prototype or bring-up path; not a supported full-kernel target.
- **Planned**: design exists, but the implementation and runtime validation do not.
- **Blocked**: implementation depends on a missing prerequisite documented below.

## Target matrix

| Capability | x86_64 UEFI | AArch64 UEFI | RISC-V 64 |
|---|---|---|---|
| Firmware entry | Implemented through bootloader API | Experimental standalone UEFI smoke image | Planned |
| Full FLX kernel entry | Implemented | Blocked on a target-neutral boot handoff and x86-only entry split | Planned |
| Serial console | COM1 | PL011 in smoke image only | Planned |
| Exception/trap path | IDT | EL1 vectors in smoke image | Planned |
| Interrupt controller | PIC/PIT legacy QEMU path | GICv3 boot-CPU timer PPI smoke path | Planned |
| Timer | PIT | Generic Timer smoke path | Planned |
| VM backend | x86_64 paging | CPU register primitives only; no AArch64 page-table backend | Planned |
| Scheduler/context switch | Experimental policy scaffolding; no production preemptive scheduler | Not integrated | Not implemented |
| SMP | Not implemented | Not implemented | Not implemented |

## Blocking technical facts

### AArch64 full-kernel boot

The production kernel entry currently registers the bootloader API entry point, and its kernel_main
uses x86_64-only CPU tables, COM1, PIC/PIT, keyboard, and x86 paging. The independent
tools/aarch64-uefi-smoke image validates the UEFI firmware path, EL1 exception vectors,
and Generic Timer/GICv3 delivery; it does **not** call the production kernel entry point.

The next implementation must:
1. Define an architecture-neutral boot handoff containing memory map, framebuffer/console,
   firmware tables, and physical-memory mapping capabilities.
2. Keep the current bootloader API entry behind cfg(target_arch = "x86_64").
3. Add a real AArch64 UEFI loader/entry that constructs the same handoff from UEFI memory
   descriptors and exits boot services safely.
4. Split common initialization from architecture-specific CPU tables, console, interrupt,
   timer, and memory-map setup.
5. Add a QEMU test that boots the actual kernel image and checks a marker emitted by the
   shared kernel initialization path.

Until those gates pass, AArch64 full-kernel boot remains experimental.

### RISC-V 64

Do not add fake/no-op CPU primitives just to make riscv64gc-unknown-none-elf compile.
A real implementation needs an explicit boot contract (OpenSBI/SBI version and privilege
level), trap-vector setup, timer source, interrupt-controller contract, and a separate QEMU
smoke test. Only then should the architecture be exposed as an experimental target.

## Issue mapping

- **#27**: not closed by the current AArch64 UEFI smoke test. Full kernel entry and QEMU
  validation are still required.
- **#29**: the Generic Timer and GICv3 path is validated only in the isolated AArch64 smoke
  image. Integration with the production kernel interrupt dispatch remains required.
- **#30**: RISC-V remains planned until the SBI/trap/timer backend and QEMU boot proof exist.
- **#31**: benchmark methodology is specified in docs/performance/benchmark-protocol.md;
  measured kernel baselines are still required before closing the issue.
- **#39**: the hybrid monolithic architecture and subsystem boundaries are documented in
  docs/design/hybrid-kernel-architecture.md. Implementation gates remain tracked by the
  architecture and subsystem tests.

## Release rule

Never describe an architecture as supported based only on cargo check, a standalone firmware
smoke image, or a source-level primitive test. Publish the exact tested image, emulator
configuration, firmware, serial markers, and CI run for every boot-support claim.
