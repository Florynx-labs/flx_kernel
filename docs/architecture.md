# FLX Kernel Architecture

FLX Kernel is a Rust `no_std` kernel developed by Florynx Labs. It is designed as a high-performance **monolithic kernel with modular internal subsystems and selective user-mode isolation**.

The normative architectural decision, subsystem boundaries, concurrency rules, DMA policy, and staged delivery gates are documented in [Hybrid Kernel Architecture](design/hybrid-kernel-architecture.md).

## Current implementation status

| Subsystem | Status | Notes |
|---|---|---|
| x86_64 UEFI boot | Implemented | QEMU/OVMF boot is validated in CI. |
| Serial and framebuffer console | Implemented | COM1 serial and UEFI framebuffer output. |
| GDT/TSS, IDT, exceptions, legacy PIC/PIT | Implemented on x86_64 | APIC/IO-APIC are not implemented. |
| Physical frame allocator | Implemented foundation | Bitmap allocator with ownership tracking and tests. |
| Kernel heap | Implemented foundation | Static early arena plus bounded frame-backed growth arenas. |
| Virtual memory | Early x86_64 foundation | Four-level page-table inspection and 4 KiB map/unmap; not a complete VM subsystem. |
| ELF64 Ring 3 launch | Experimental | Single-task launch and write syscall smoke test; no scheduler or reaper. |
| Scheduler | Policy scaffold | Round-robin policy model only; no production context switch/preemption. |
| Architecture-neutral CPU contract | In progress | x86_64 and AArch64 implementations; shared spinlock and paging operations use the facade. |
| AArch64 | Experimental compile check | CPU primitives cross-compile; no UEFI kernel boot, exception vectors, timer, or GIC integration. |
| RISC-V 64 | Planned | No implementation or boot proof yet. |
| VFS, storage, networking, production IPC/capabilities | Not implemented | Roadmap only; no support claim. |
| SMP and IOMMU/DMA isolation | Not implemented | Requires per-CPU state, shootdowns, and device ownership. |

## Architecture support policy

- x86_64 is the only boot-validated kernel target today.
- AArch64 cross-compilation is not equivalent to a successful boot.
- RISC-V and other architectures remain planned until real architecture-specific entry, exception, timer, and QEMU boot tests exist.
- Generic policy code must not issue architecture-specific privileged instructions directly.
- Unsupported operations must fail explicitly; do not substitute no-op implementations.

## Roadmap

1. Keep x86_64 UEFI, paging, Ring 3, and CI regression coverage green.
2. Complete the architecture HAL and validate an AArch64 UEFI boot path.
3. Add AArch64 exception vectors, Generic Timer, and GIC support with QEMU tests.
4. Complete task/thread context lifecycle and safe address-space ownership before preemption/SMP.
5. Add versioned syscall and capability-checked IPC contracts.
6. Add VFS, storage, networking, and a constrained driver model.
7. Introduce RISC-V only with an independently validated boot path.
8. Establish reproducible performance baselines before tuning.

Each stage must distinguish compile checks, boot checks, and runtime feature tests. See the [hybrid architecture document](design/hybrid-kernel-architecture.md) for acceptance criteria.
