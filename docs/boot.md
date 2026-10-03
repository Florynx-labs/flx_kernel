# FLX Kernel — Boot Subsystem Documentation

## 1. Overview
The boot subsystem of FLX Kernel coordinates early system bring-up under UEFI on `x86_64` hardware.

## 2. Boot Flow
1. **UEFI Firmware Bring-Up**:
   - The platform loads UEFI firmware (or OVMF under QEMU).
   - UEFI discovers bootable media containing the EFI system partition (`ESP`).
2. **Bootloader Invocation**:
   - The bootloader EFI binary (`BOOTX64.EFI`) is executed.
   - Bootloader queries UEFI Boot Services for:
     - GOP (Graphics Output Protocol) -> linear framebuffer address, resolution, stride, format.
     - UEFI Memory Map -> table of physical memory descriptors.
     - ACPI tables -> RSDP address.
   - Prepares identity page tables and loads the kernel ELF binary into 64-bit long mode.
   - Exits UEFI Boot Services and invokes the kernel entry point: `kernel_main(&'static mut BootInfo)`.
3. **Kernel Initialization**:
   - `drivers::serial::SERIAL1.lock().init()`: Initializes COM1 at 115200 baud, 8N1.
   - `framebuffer::init()`: Sets up the 8x16 monospace console.
   - `logging::init()`: Enables simultaneous logging to COM1 and the graphical display.
   - `arch::x86_64::cpu::get_cpu_vendor()`: Queries CPUID for processor vendor and model.
   - `boot::KernelBootInfo::parse()`: Aggregates physical memory regions into usable/reserved counts.
   - Enters safe CPU halt loop awaiting interrupts.
