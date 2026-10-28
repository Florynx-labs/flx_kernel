# FLX Kernel Architecture Specification

## 1. Vision & Core Philosophy

**FLX Kernel** is a modern, modular, general-purpose operating system kernel written in Rust (`no_std`). Developed by **Florynx Labs**, it is designed from day one to serve as the core technological foundation for a family of operating systems:

* **FLX Desktop**: Modern interactive workstation OS.
* **FLX Server**: High-throughput, secure server OS.
* **FLX Lite / Embedded**: Low-footprint, real-time capable systems.
* **FLX Dev / Edu**: Development, research, and educational platforms.

### Core Principles
1. **Safety First**: Memory and concurrency safety guaranteed by Rust strong types and documented `unsafe` invariants.
2. **Zero Fakes**: No simulated subsystems. What exists is real; what is not yet implemented is clearly labeled `STATUS: NOT IMPLEMENTED`.
3. **Hardware-First**: Built from the metal up targeting standard UEFI on `x86_64`.
4. **Clean Abstractions**: Clean separation between architecture-specific low-level code, core subsystems, drivers, and userland.
5. **Observability**: First-class serial diagnostics, high-resolution framebuffer console, and structured logging.

---

## 2. High-Level System Architecture

```
+-------------------------------------------------------------------------+
|                              FLX USERLAND                               |
|       FLX Init   |   FLX Shell   |   FLX Core Utilities   |   Apps      |
+-------------------------------------------------------------------------+
                                    | (Syscall ABI)
+-----------------------------------+-------------------------------------+
|                              FLX KERNEL                                 |
|                                                                         |
|  +--------------------+  +---------------------+  +------------------+  |
|  | Process & Threads  |  |     VFS & Storage   |  |   Network Stack  |  |
|  | (Future Milestone) |  | (Future Milestone)  |  | (Future Milest.) |  |
|  +--------------------+  +---------------------+  +------------------+  |
|                                                                         |
|  +--------------------+  +---------------------+  +------------------+  |
|  | Task Scheduler     |  | Virtual & Physical  |  | Security & Caps  |  |
|  | (Future Milestone) |  | Memory Management   |  | (Future Milest.) |  |
|  +--------------------+  +---------------------+  +------------------+  |
|                                                                         |
|  +-------------------------------------------------------------------+  |
|  |                         Kernel Core Services                      |  |
|  |   Logging Subsystem  |  Spinlocks & Sync  |  Panic & Diagnostics  |  |
|  +-------------------------------------------------------------------+  |
|                                                                         |
|  +--------------------+  +---------------------+  +------------------+  |
|  | 16550 UART Serial  |  | Framebuffer Console |  | Timers & PIC/APIC|  |
|  | Driver (COM1)      |  | 8x16 Monospace Font |  | (PIC/PIT: v0.2)  |  |
|  +--------------------+  +---------------------+  +------------------+  |
|                                                                         |
|  +-------------------------------------------------------------------+  |
|  |               x86_64 Architecture Layer (arch/x86_64)             |  |
|  |        CPUID  |    GDT/IDT       |  Port I/O  |  CPU Control      |  |
|  +-------------------------------------------------------------------+  |
+-------------------------------------------------------------------------+
                                    ^
                                    | Boot Info (Memory Map, Framebuffer)
+-----------------------------------+-------------------------------------+
|                      UEFI Bootloader (FLX Boot)                         |
|                 x86_64 UEFI Firmware / OVMF (QEMU)                      |
+-------------------------------------------------------------------------+
                                    ^
+-----------------------------------+-------------------------------------+
|                         Physical Hardware                               |
|                  x86_64 CPU, RAM, Serial, Display                       |
+-------------------------------------------------------------------------+
```

---

## 3. Subsystem Breakdown (v0.2.0 + v0.3.0 foundations)

| Subsystem | Location | Status | Description |
|-----------|----------|--------|-------------|
| **Entry & Boot** | `kernel/src/main.rs`, `kernel/src/boot/` | **READY** | UEFI boot protocol handler, BootInfo parser, memory map summary. |
| **Serial Driver** | `kernel/src/drivers/serial.rs` | **READY** | Standard 16550 UART driver on COM1 (`0x3F8`), 115200 baud, 8N1. |
| **Framebuffer** | `kernel/src/framebuffer/` | **READY** | Linear RGB/BGR framebuffer console, 8x16 font, text scrolling. |
| **Logging** | `kernel/src/logging.rs` | **READY** | Dual-output logging (Serial with ANSI colors, FB with RGB colors). |
| **Panic Handler** | `kernel/src/panic.rs` | **READY** | Lock-safe, halts CPU, outputs location and panic diagnostic banner. |
| **Sync Primitives**| `kernel/src/sync/` | **READY** | Bare-metal IRQ-safe `Spinlock<T>` (disables local interrupts while held). |
| **Arch Support** | `kernel/src/arch/x86_64/` | **READY** | CPUID, port I/O, interrupt control, `hlt` loop. |
| **GDT / TSS** | `kernel/src/arch/x86_64/gdt.rs` | **READY** | Flat 64-bit segments (kernel/user), TSS with IST stacks for #DF and NMI. |
| **IDT & Interrupts** | `kernel/src/arch/x86_64/{idt,interrupts}.rs` | **READY** | 256 assembly stubs → uniform `InterruptFrame` → Rust dispatcher; exception dumps; handler registry. Stub table uses PC-relative offsets (PIE-safe). |
| **PIC** | `kernel/src/drivers/pic.rs` | **READY** | 8259 remapped to vectors 32–47, masking, EOI, spurious IRQ filtering. Local APIC / IO-APIC: **NOT IMPLEMENTED**. |
| **Timer** | `kernel/src/drivers/pit.rs` | **READY** | PIT channel 0 at 100 Hz, tick counter, uptime. |
| **Keyboard** | `kernel/src/drivers/keyboard.rs` | **READY** | PS/2 IRQ 1, scancode set 1 US layout, Shift/Caps Lock, ring buffer. |
| **Physical Memory** | `kernel/src/mm/frame.rs` | **FOUNDATION READY** | Bitmap frame allocator over the UEFI memory map, double-free detection, self-test. |
| **Paging** | `kernel/src/mm/paging.rs` | **EARLY FOUNDATION** | Reads active CR3; translates 4 KiB / 2 MiB / 1 GiB mappings; maps/unmaps 4 KiB leaves; invalidates TLB. Not yet a complete address-space manager. |
| **Virtual Range Allocation** | `kernel/src/mm/virtual_range.rs` | **FOUNDATION READY** | Fixed-capacity bitmap allocator for contiguous canonical virtual-page ranges. Reserves addresses only; does not create page mappings. |
| **Kernel Heap** | `kernel/src/mm/` | **NOT IMPLEMENTED** | Requires a defined safe virtual arena, paging integration, allocation metadata, and failure handling. |

---

## 4. Roadmap

* **v0.1.0**: UEFI Boot, x86_64, Framebuffer, Serial COM1, Panic Handler, Logging, Memory Map.
* **v0.2.0**: GDT, IDT, CPU exceptions, legacy PIC interrupts (timer, keyboard), physical frame allocator.
* **v0.3.0 (in progress)**: Active paging inspection and 4 KiB map/unmap primitives; virtual-range reservation. Next: safe kernel virtual arena, mapping tests, kernel heap and allocation diagnostics.
* **v0.4.0**: Task structures, saved CPU contexts, kernel stacks, scheduler design, then cooperative context switching before preemption.
* **v0.5.0**: User/kernel privilege boundary, per-process address spaces, syscall ABI, process lifecycle and isolation tests.
* **v0.6.0**: VFS interfaces, initramfs/ramdisk, FAT support and initial user program.
* **v0.7.0**: FLX init and shell, basic user utilities, user-space runtime and system-call wrappers.
* **v0.8.0**: PCI enumeration, storage drivers, VirtIO block and console devices, driver model foundations.
* **v0.9.0**: Network interfaces, Ethernet, ARP, IPv4, ICMP, UDP and TCP with protocol tests.
* **v1.0.0**: SMP, local APIC/IO-APIC, capability-oriented security foundations, hardening and FLX OS release candidate.

These milestones are a roadmap, not claims that the future subsystems already exist. Each milestone requires automated tests and a successful QEMU boot before it is considered complete.


## Multi-architecture roadmap (incremental)

FLX remains **bootable on x86_64 only** today. Architecture support will be added in explicit, testable stages rather than claiming a CPU target is supported merely because it compiles.

| Architecture | Current stage | Next proof point |
|---|---|---|
| x86_64 | Existing UEFI boot and QEMU smoke tests | Stabilize Ring 3 transition and isolate x86-specific services |
| AArch64 / ARM64 | CPU primitive scaffold and cross-compilation check | UEFI/EDK2 entry, exception vectors, generic timer, GIC, and QEMU `virt` boot |
| RISC-V 64 | Planned | SBI boot path, trap entry, CLINT/ACLINT or platform timer, PLIC/AIA, QEMU `virt` boot |
| LoongArch64 | Research backlog | Toolchain/firmware feasibility and minimal serial boot before committing to support |

### Portability rules

- Keep CPU control, exception entry, interrupt controllers, timers, and context switching under `kernel/src/arch/<arch>/`.
- Move common memory, scheduler, process, and syscall policies behind architecture-neutral interfaces; do not make one architecture emulate another's registers or interrupt model.
- Preserve a per-architecture boot and QEMU smoke test. Cross-compilation is a gate, not proof of successful hardware boot.
- Prefer feature detection and conservative fallbacks over assuming optional CPU extensions.

### Performance and security principles

The design will study upstream Linux 7.x architecture/MM documentation and Microsoft's published Windows kernel engineering work as references, not copy their proprietary implementation. Candidate principles include per-CPU data and reduced global-lock contention, cache-aware allocation, batching expensive TLB operations, heterogeneous-core-aware scheduling when a scheduler exists, and zero-initialized allocations by default. Each optimization must be benchmarked and retain correctness/security tests; no speculative tuning before measurements.
