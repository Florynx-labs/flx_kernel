# FLX Kernel: hybrid monolithic architecture

**Status:** architecture decision and staged implementation plan  
**Scope:** kernel architecture, not a claim that every listed subsystem exists today.

## 1. Architectural decision

FLX Kernel is a **monolithic kernel with modular internal subsystems and selective user-mode isolation**.

The privileged core owns latency-sensitive and security-critical primitives: CPU/interrupt entry, scheduler, virtual and physical memory, syscall dispatch, core IPC, VFS coordination, and capability enforcement. Subsystems have explicit interfaces and ownership rules. Components that benefit from restartability or fault isolation may later run as user-mode services or drivers, provided the kernel has real IPC, memory isolation, capability checks, and device isolation.

This is not a microkernel rewrite. Nor is it permission to build an unstructured monolith.

## 2. Target layering

```text
User applications
       |
Versioned syscall ABI + handles/capabilities
       |
Optional user-mode services and isolated drivers
       |             shared-memory queues + validated IPC
       +---------------------+
                             |
+------------------------------------------------------+
|                 Privileged kernel core               |
| scheduler | VM | syscall entry | IPC core | VFS core |
| capability checks | timers | interrupt dispatch     |
+------------------------------------------------------+
       | stable internal subsystem interfaces
+------------------------------------------------------+
| Drivers and architecture backends                    |
| x86_64 | AArch64 | RISC-V (experimental/planned)      |
+------------------------------------------------------+
       |
Firmware / hardware
```

## 3. Subsystem boundaries

### 3.1 Architecture HAL

Architecture-specific instructions, register layouts, exception entry, interrupt controllers, timers, context switching, and address-space activation belong under `kernel/src/arch/<arch>/`.

Common code must call explicit contracts. A capability must not appear available on a target unless that target has a real implementation. Compile-only checks are useful but do not establish boot support.

Current state:
- **Implemented:** x86_64 CPU controls, GDT/TSS, IDT, exception/interrupt dispatch, syscall entry, UEFI boot.
- **Experimental:** AArch64 CPU primitives and a small architecture contract cross-check.
- **Not implemented:** AArch64 UEFI boot, vector table, Generic Timer/GIC integration; RISC-V boot and trap path.

### 3.2 Memory management

The memory subsystem owns physical frame allocation, page-table structures, virtual-range reservations, kernel heap arenas, and user address-space ownership. Page-table mutations must specify ownership, permissions, TLB invalidation, and rollback behavior.

Current state:
- **Implemented foundations:** x86_64 bitmap frame allocator, early/growing heap, 4 KiB mapping operations, user address-space creation and ELF loading.
- **Not implemented:** complete multi-process address-space lifecycle, SMP TLB shootdowns, per-CPU address-space tracking, IOMMU-backed DMA isolation.

### 3.3 Tasks and scheduling

Task/thread objects own saved context, kernel stack, address-space reference, state, and scheduling metadata. Scheduler policy stays architecture-neutral; low-level context save/restore is delegated to the HAL.

Current state:
- **Experimental:** scheduler policy scaffolding and a single-task Ring 3 launch path.
- **Not implemented:** production context switching, preemption, process teardown/reaping, SMP scheduling, CPU affinity.

### 3.4 IPC and capabilities

The kernel IPC core validates endpoint ownership, message sizes, and transferred handles. Capabilities are unforgeable kernel-validated references with explicit rights; raw user pointers are never authority. Shared memory is mapped only after access checks and has an explicit lifetime/teardown contract.

Current state: **not implemented** as a production subsystem. The current syscall smoke test is not a general IPC or capability system.

### 3.5 VFS, storage, and network

Keep VFS namespace and object lifetime policy architecture-neutral. Filesystems and device drivers implement narrow interfaces. Storage and network drivers may move to user mode only after DMA isolation and restartable IPC are real.

Current state: **not implemented**.

## 4. Concurrency and SMP

Before enabling multiple CPUs:
1. Add per-CPU storage and identify CPUs through the architecture contract.
2. Define interrupt-safe lock rules, lock ordering, and NMI/FIQ restrictions.
3. Add scheduler run queues and CPU affinity.
4. Implement remote TLB shootdown acknowledgements and safe address-space destruction.
5. Measure lock contention and interrupt latency under repeatable workloads.

A local interrupt mask is not an SMP lock. Atomics and locks must provide cross-CPU mutual exclusion, and disabling interrupts only prevents same-CPU interrupt re-entry.

## 5. DMA and device isolation

A driver is not isolated merely because it is placed in user mode. Before untrusted device drivers are supported, FLX needs:
- DMA buffer ownership and pin/unpin lifetimes;
- IOMMU domain setup and teardown where hardware supports it;
- bounded, validated device command interfaces;
- interrupt delivery mediated by the kernel;
- recovery behavior for driver exit or restart.

Until then, drivers requiring unrestricted DMA remain trusted kernel components.

## 6. Performance policy

Do not optimize based on intuition alone. Establish reproducible baselines for:
- frame allocation/free throughput and tail latency;
- address-space activation and TLB invalidation cost;
- lock acquisition latency and contention;
- interrupt-to-handler latency;
- scheduler dispatch latency once context switching exists.

Record CPU model, QEMU version/configuration, memory size, compiler/toolchain, iteration count, and percentile statistics. Every optimization must include correctness tests and before/after measurements. Host-side microbenchmarks must not be presented as hardware/kernel measurements.

## 7. Architecture support matrix

| Target | Compile check | Real boot | Runtime status |
|---|---|---|---|
| x86_64 UEFI | CI | QEMU/OVMF CI | Ring 3 ELF + write syscall smoke test |
| AArch64 | CPU primitive cross-check | Not yet | No kernel boot claim |
| RISC-V 64 | Not yet | Not yet | Planned |
| LoongArch64 | Not yet | Not yet | Research only |

## 8. Delivery gates

A subsystem is complete only when:
- its contract and ownership/lifetime rules are documented;
- unsupported operations fail explicitly;
- unit/contract tests cover success and failure paths;
- target-specific code is isolated behind the HAL;
- bootable targets have a real QEMU smoke test;
- performance claims include reproducible measurements.

Cross-compilation is not boot validation. A prototype is not a production process manager. A future roadmap item is not an implemented feature.
