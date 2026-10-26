<p align="center">
  <img src="https://raw.githubusercontent.com/Florynx-labs/flx_kernel/main/asset/flx-cover.png" alt="FLX Kernel — Florynx Labs" width="100%"/>
</p>

<p align="center">
  <strong>FLX Kernel</strong> &mdash; A modern, modular, high-performance general-purpose OS kernel in Rust <code>no_std</code><br/>
  Developed by <strong>Florynx Labs</strong>
</p>

<p align="center">
  <a href="https://github.com/Florynx-labs/flx_kernel/actions/workflows/ci.yml"><img src="https://github.com/Florynx-labs/flx_kernel/actions/workflows/ci.yml/badge.svg" alt="CI"/></a>
  <img src="https://img.shields.io/badge/version-v0.2.0-blue?style=flat-square" alt="v0.2.0"/>
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

## Current Status (v0.2.0 + v0.3.0 memory foundations)

- **Target**: `x86_64-unknown-none`
- **Boot Protocol**: UEFI (64-bit Long Mode, OVMF)
- **Runtime**: Pure Rust `#![no_std]` — zero C runtime dependency
- **Console Output**:
  - Serial 16550 UART COM1 (115200 baud, 8N1) with ANSI colors
  - Graphical Framebuffer Console (GOP, 8×16 monospace font, 24-bit RGB colors, automatic scrolling)
- **Diagnostics**:
  - Panic handler with file/line/column location, CPU state dump, and guaranteed CPU halt
  - Structured dual-target logging: `[OK]` `[INFO]` `[WARN]` `[ERROR]` `[DEBUG]`
  - CPU exception handler with full register dump (page fault reports CR2 and error bits)
- **CPU tables**: GDT + 64-bit TSS (IST stacks for double fault and NMI), IDT with 256 vectors
- **Interrupts**: 8259 PIC remapped to vectors 32–47 (spurious IRQ handling), PIT system timer at 100 Hz, PS/2 keyboard (scancode set 1, US layout, ring buffer)
- **Physical memory**: UEFI memory map parsing and a bitmap **physical frame allocator** (4 KiB frames, double-free detection, protection against freeing reserved frames, boot-time self-test)
- **User-mode foundation**: initial Ring 3 IRETQ transition, bounded four-page user stack preparation, dedicated single-task ring-0 interrupt stack, and a DPL3 `int 0x80` gate; not yet a scheduler or production process manager
- **Syscall foundation**: minimal `getpid` (PID 1), an exit-request flag, explicit `ENOSYS` for unsupported calls, and a bounded `write` to serial output (up to 1 KiB per call) using validated user-page ownership and a kernel-side copy; full process teardown and scheduling are not implemented
- **Virtual memory foundation**: inspect active CR3 tables; translate 4 KiB / 2 MiB / 1 GiB mappings; map and unmap 4 KiB pages; invalidate TLB after edits; propagate writable/user/NX requirements through intermediate entries; create isolated user address spaces with private lower-half page tables and shared supervisor-only kernel mappings
- **ELF64 userspace launch foundation**: bounded parser and transactional PT_LOAD mapper for static little-endian x86_64 ET_EXEC images; copies segment bytes into isolated user pages, leaves BSS zero-filled, applies user/write/NX permissions, and rolls back newly mapped pages on load failure. `mm::process::UserProcess` now prepares an isolated address space, loads the ELF, builds a four-page user stack, and exposes an explicit unsafe one-way Ring 3 launch; it is not automatically invoked during boot and has no scheduler or process reaper yet
- **Kernel heap**: a thread-safe first-fit allocator with a 1 MiB static early arena plus on-demand, zeroed, frame-backed heap arenas mapped into a probed virtual window after the physical frame allocator is ready; bounded arena count and size, allocation routing, and boot-time smoke tests
- **Virtual range allocator**: fixed-capacity, bitmap-backed contiguous virtual-page reservation with bounds, alignment, canonical-address, and double-free checks; does not map pages by itself
- **Sync**: IRQ-safe `Spinlock<T>` (local interrupts are disabled while the lock is held)
- **Tests / CI**: Python source-contract regression tests and Rust host-side memory tests run before the kernel build and headless QEMU/OVMF boot validation

**Not implemented yet**: multi-task scheduling, safe user-pointer copy-in/copy-out, complete process exit/reaping semantics, safe user-pointer copy-in/copy-out, complete process exit/reaping semantics, and a production syscall ABI; heap arenas are bounded and independently managed (they do not yet form one contiguous expandable heap); Local APIC / IO-APIC and SMP, VFS/filesystems, networking, and a broad driver model. These are planned in staged milestones; the current paging API is an early foundation, not complete virtual memory management.

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
│       ├── arch/x86_64/        # CPUID, port I/O, GDT/TSS, IDT, interrupt stubs & dispatch
│       ├── boot/               # BootInfo & UEFI memory map parsing
│       ├── drivers/            # 16550 UART, 8259 PIC, PIT timer, PS/2 keyboard
│       ├── framebuffer/        # Linear framebuffer writer & 8×16 bitmap font
│       ├── mm/                 # Frame allocator, paging, virtual-range allocator
│       └── sync/               # Bare-metal IRQ-safe Spinlock<T>
├── tools/
│   ├── runner/                 # Host tool to build UEFI disk & run QEMU
│   ├── mm-tests/               # Host-side Rust tests for memory-management algorithms
│   ├── tests/                  # Python regression tests for allocator safety contracts
│   └── ci/                     # Boot validation script used by CI
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

> The `bootloader` crate's build script uses `-Z build-std`, which requires a nightly cargo.
> On the pinned stable toolchain, set `RUSTC_BOOTSTRAP=1` (as CI does), e.g.
> `RUSTC_BOOTSTRAP=1 cargo run --package flx-runner`
> (PowerShell: `$env:RUSTC_BOOTSTRAP=1; cargo run --package flx-runner`).
> OVMF firmware is downloaded automatically into `target/ovmf`.

### Run memory-management tests

```bash
cargo test --package flx-mm-tests
python3 -m unittest discover -s tools/tests -v
```

The Rust harness tests the production memory-management modules on the host, including paging permission helpers and user-address validation, virtual-range management, physical frames, heap allocation, and heap growth sizing/initialization. The Python suite guards key physical-frame allocator safety contracts, including ownership tracking, address validation, and free-frame accounting.

### Headless boot test (same check as CI)

```bash
RUSTC_BOOTSTRAP=1 tools/ci/check_boot.sh 150
```

### Ring 3 ELF smoke test (opt-in)

Run a minimal static ELF in Ring 3 under QEMU and verify that its `int 0x80` `write`
syscall emits `FLX USERSPACE: PASS` to COM1:

```bash
RUSTC_BOOTSTRAP=1 tools/ci/check_userspace_smoke.sh 150
```

This is a bring-up test, not a process manager: the payload spins after writing and the
current `SYS_EXIT` path does not yet switch back to the kernel scheduler.

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

Serial console (framebuffer shows the same lines):

```
==================================================
FLX KERNEL v0.2.0 (Florynx Labs)
Architecture: x86_64 (AuthenticAMD)
Boot:         UEFI
Runtime:      Rust no_std
==================================================
[OK   ] Boot
[OK   ] CPU (AuthenticAMD)
[OK   ] Memory map (105 regions, 500 MB usable / 12803 MB total)
[OK   ] Framebuffer
[OK   ] GDT + TSS (IST: double fault, NMI)
[WARN ] Breakpoint at RIP=0x000001000000d425
[OK   ] IDT + CPU exceptions (breakpoint self-test)
[OK   ] Frame allocator (130804 frames tracked, 499 MB free)
[OK   ] Frame allocator self-test
[OK   ] Timer IRQ (PIT 100 Hz, 10 ticks, uptime 100 ms)
[OK   ] Keyboard IRQ (PS/2, press a key to test)
[OK] Kernel initialized.
```

The `[WARN] Breakpoint` line is expected: it is the deliberate `int3` self-test of the IDT.
Region counts, memory sizes and the CPU vendor depend on the host/QEMU configuration.

---

## Roadmap

| Milestone | Feature |
|---|---|
| **v0.1.0** ✅ | UEFI Boot, Framebuffer, Serial, Panic, Logging, Memory Map |
| **v0.2.0** ✅ | GDT, IDT, CPU Exceptions, Interrupts (Timer, Keyboard), Physical Frame Allocator |
| **v0.3.0** | Virtual Memory Paging, Kernel Heap, Dynamic Allocator |
| **v0.4.0** | Cooperative & Preemptive Scheduler, Tasks, Context Switching |
| **v0.5.0** | Processes, Threads, User/Kernel Separation, Syscall ABI |
| **v0.6.0** | VFS, Ramdisk/FAT32, Initial Userspace |
| **v1.0.0** | Multicore SMP, Capabilities Security, FLX OS Release Candidate |

---

## License

Licensed under the **MIT License** — see [`LICENSE`](LICENSE) for details.

Copyright © 2026 **Florynx Labs**
