<p align="center">
  <img src="https://raw.githubusercontent.com/Florynx-labs/flx_kernel/main/asset/flx-cover.png" alt="FLX Kernel — Florynx Labs" width="100%"/>
</p>

<p align="center">
  <strong>FLX Kernel</strong> &mdash; A modern, modular, high-performance general-purpose OS kernel in Rust <code>no_std</code><br/>
  Developed by <strong>Florynx Labs</strong>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/version-v0.1.0-blue?style=flat-square" alt="v0.1.0"/>
  <img src="https://img.shields.io/badge/arch-x86__64-lightgrey?style=flat-square" alt="x86_64"/>
  <img src="https://img.shields.io/badge/boot-UEFI-orange?style=flat-square" alt="UEFI"/>
  <img src="https://img.shields.io/badge/runtime-Rust_no__std-red?style=flat-square" alt="Rust no_std"/>
  <img src="https://img.shields.io/badge/license-MIT-green?style=flat-square" alt="MIT"/>
</p>

---

## Vision

FLX Kernel is engineered as a foundational technology base designed to evolve into several operating system distributions of the **FLX OS** ecosystem:

| Distribution | Purpose |
|---|---|
| **FLX Desktop** | Modern interactive workstation OS |
| **FLX Server** | High-throughput, secure server platform |
| **FLX Lite / Embedded** | Low-footprint, real-time capable systems |
| **FLX Dev / Edu** | Development, research, and educational platforms |

---

## Current Status (Milestone v0.1.0)

- **Target**: `x86_64-unknown-none`
- **Boot Protocol**: UEFI (64-bit Long Mode, OVMF)
- **Runtime**: Pure Rust `#![no_std]` — zero C runtime dependency
- **Console Output**:
  - Serial 16550 UART COM1 (115200 baud, 8N1) with ANSI colors
  - Graphical Framebuffer Console (GOP, 8×16 monospace font, 24-bit RGB colors, automatic scrolling)
- **Diagnostics**:
  - Panic handler with file/line/column location, CPU state dump, and guaranteed CPU halt
  - Structured dual-target logging: `[OK]` `[INFO]` `[WARN]` `[ERROR]` `[DEBUG]`
- **Memory Map**: Full UEFI physical memory map parsing and region summary reporting

---

## Repository Structure

```
flx-kernel/
├── Cargo.toml                  # Workspace manifest
├── rust-toolchain.toml         # Rust toolchain pinning (stable)
├── README.md                   # Project overview & quickstart
├── LICENSE                     # MIT License
├── asset/                      # Project assets (logo, cover)
├── kernel/                     # Kernel core (no_std, x86_64-unknown-none)
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs             # Kernel entry point
│       ├── panic.rs            # Kernel panic diagnostics & CPU halt
│       ├── logging.rs          # Dual serial & framebuffer logging macros
│       ├── arch/x86_64/        # x86_64 CPU CPUID, port I/O, asm primitives
│       ├── boot/               # BootInfo & UEFI memory map parsing
│       ├── drivers/            # Hardware drivers (16550 UART serial)
│       ├── framebuffer/        # Linear framebuffer writer & 8×16 bitmap font
│       └── sync/               # Bare-metal Spinlock<T>
├── tools/
│   └── runner/                 # Host tool to build UEFI disk & run QEMU
└── docs/                       # Technical documentation
    ├── architecture.md         # Global architecture & roadmap
    └── boot.md                 # UEFI boot protocol documentation
```

---

## Quickstart

### Prerequisites
- **Rust stable** with target `x86_64-unknown-none`
- **QEMU** (`qemu-system-x86_64`)

```bash
rustup target add x86_64-unknown-none
```

### Build the Kernel

```bash
cargo build --package flx-kernel --target x86_64-unknown-none
```

### Build & Run under QEMU (UEFI + OVMF)

```bash
cargo run --package flx-runner
```

### Build UEFI disk image only (no QEMU launch)

```bash
cargo run --package flx-runner -- --no-run
```

The bootable UEFI disk image is generated at:
```
target/flx-kernel-uefi-debug.img
```

---

## Expected Boot Output

```
FLX KERNEL
-------------------------
Version:      0.1.0
Architecture: x86_64 (GenuineIntel)
Boot:         UEFI
Runtime:      Rust no_std

[OK   ] Boot
[OK   ] CPU (GenuineIntel)
[OK   ] Memory map (8 regions, 504 MB usable / 512 MB total)
[OK   ] Framebuffer

Kernel initialized.
```

---

## Roadmap

| Milestone | Feature |
|---|---|
| **v0.1.0** ✅ | UEFI Boot, Framebuffer, Serial, Panic, Logging, Memory Map |
| **v0.2.0** | GDT, IDT, CPU Exceptions, Interrupts (Timer, Keyboard), Physical Frame Allocator |
| **v0.3.0** | Virtual Memory Paging, Kernel Heap, Dynamic Allocator |
| **v0.4.0** | Cooperative & Preemptive Scheduler, Tasks, Context Switching |
| **v0.5.0** | Processes, Threads, User/Kernel Separation, Syscall ABI |
| **v0.6.0** | VFS, Ramdisk/FAT32, Initial Userspace |
| **v1.0.0** | Multicore SMP, Capabilities Security, FLX OS Release Candidate |

---

## License

Licensed under the **MIT License** — see [`LICENSE`](LICENSE) for details.

Copyright © 2026 **Florynx Labs**
