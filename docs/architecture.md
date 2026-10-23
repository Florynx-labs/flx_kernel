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

## 3. Subsystem Breakdown (Milestone v0.2.0)

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
| **Physical Memory** | `kernel/src/mm/frame.rs` | **READY** | Bitmap frame allocator over the UEFI memory map, double-free detection, self-test. |

---

## 4. Roadmap

* **v0.1.0**: UEFI Boot, x86_64, Framebuffer, Serial COM1, Panic Handler, Logging, Memory Map.
* **v0.2.0 (Current)**: GDT, IDT, CPU Exceptions, Interrupts (Timer, Keyboard), Physical Frame Allocator.
* **v0.3.0**: Virtual Memory Paging, Kernel Heap, Dynamic Memory Allocator.
* **v0.4.0**: Cooperative & Preemptive Scheduler, Tasks, Context Switching.
* **v0.5.0**: Processes, Threads, User/Kernel Separation, Syscall ABI.
* **v0.6.0**: VFS, Ramdisk/FAT32, Initial Userspace ("Hello World").
* **v0.7.0**: FLX Shell, Standard C/Rust Libc, Basic Utilities.
* **v0.8.0**: Storage & PCI Drivers, VirtIO.
* **v0.9.0**: Network Stack (Ethernet, ARP, IPv4, UDP, TCP).
* **v1.0.0**: Multicore SMP, Capabilities Security, FLX OS First Release Candidate.
