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
| AArch64 bring-up | Experimental | Separate UEFI first-stage image exercises production CPU primitives, EL1 vectors, and Generic Timer/GICv3 smoke path; this is not a full FLX kernel boot. |
| RISC-V 64 | Planned | No implementation or boot proof yet. |
| VFS, storage, networking, production IPC/capabilities | Not implemented | Roadmap only; no support claim. |
| SMP and IOMMU/DMA isolation | Not implemented | Requires per-CPU state, shootdowns, and device ownership. |

## Architecture support policy

- x86_64 is the only full FLX kernel target boot-validated today.
- AArch64 has a separate first-stage UEFI/QEMU bring-up image; it does not boot the full kernel or establish full-target support.
- AArch64 runtime support claims require the dedicated QEMU smoke job to pass, including the controlled exception and periodic timer markers.
- RISC-V and other architectures remain planned until real architecture-specific entry, exception, timer, and QEMU boot tests exist.
- Generic policy code must not issue architecture-specific privileged instructions directly.
- Unsupported operations must fail explicitly; do not substitute no-op implementations.

## Roadmap

1. Keep x86_64 UEFI, paging, Ring 3, and CI regression coverage green.
2. Complete the architecture HAL and promote the AArch64 first-stage bring-up into a full-kernel UEFI boot path.
3. Integrate AArch64 exception vectors, Generic Timer, and GIC support with the full kernel and retain independent QEMU runtime tests.
4. Complete task/thread context lifecycle and safe address-space ownership before preemption/SMP.
5. Add versioned syscall and capability-checked IPC contracts.
6. Add VFS, storage, networking, and a constrained driver model.
7. Introduce RISC-V only with an independently validated boot path.
8. Establish reproducible performance baselines before tuning.

Each stage must distinguish compile checks, boot checks, and runtime feature tests. See the [hybrid architecture document](design/hybrid-kernel-architecture.md) for acceptance criteria.
