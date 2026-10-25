//! Global Descriptor Table (GDT) and Task State Segment (TSS).
//!
//! In long mode segmentation is mostly flat, but the GDT is still required for the
//! kernel/user code and data selectors, and the TSS provides the Interrupt Stack Table
//! (IST) used to run critical handlers (double fault, NMI) on known-good stacks.
//!
//! Layout (selectors):
//! ```text
//! 0x00  null
//! 0x08  kernel code (64-bit)
//! 0x10  kernel data
//! 0x18  user data   (RPL 3 -> 0x1B)   \  order required by SYSRET (STAR MSR)
//! 0x20  user code   (RPL 3 -> 0x23)   /
//! 0x28  TSS (16-byte descriptor, occupies two slots)
//! ```

use core::arch::asm;
use core::mem::size_of;
use core::ptr::addr_of_mut;

pub const KERNEL_CODE_SELECTOR: u16 = 0x08;
pub const KERNEL_DATA_SELECTOR: u16 = 0x10;
pub const USER_DATA_SELECTOR: u16 = 0x18 | 3;
pub const USER_CODE_SELECTOR: u16 = 0x20 | 3;
const TSS_SELECTOR: u16 = 0x28;

/// IST slot (1-based, as encoded in the IDT) used for the double fault handler.
pub const DOUBLE_FAULT_IST: u8 = 1;
/// IST slot (1-based) used for the NMI handler.
pub const NMI_IST: u8 = 2;

const IST_STACK_SIZE: usize = 4096 * 5;

#[repr(C, align(16))]
struct IstStack([u8; IST_STACK_SIZE]);

static mut DOUBLE_FAULT_STACK: IstStack = IstStack([0; IST_STACK_SIZE]);
static mut NMI_STACK: IstStack = IstStack([0; IST_STACK_SIZE]);

/// 64-bit Task State Segment (Intel SDM Vol. 3, 7.7).
#[repr(C, packed)]
struct Tss {
    reserved0: u32,
    /// Stack pointers for privilege levels 0..=2.
    rsp: [u64; 3],
    reserved1: u64,
    /// Interrupt Stack Table pointers (IST1..IST7).
    ist: [u64; 7],
    reserved2: u64,
    reserved3: u16,
    /// Offset of the I/O permission bitmap; `size_of::<Tss>()` means "none".
    iomap_base: u16,
}

static mut TSS: Tss = Tss {
    reserved0: 0,
    rsp: [0; 3],
    reserved1: 0,
    ist: [0; 7],
    reserved2: 0,
    reserved3: 0,
    iomap_base: size_of::<Tss>() as u16,
};

/// Number of 8-byte GDT slots (the TSS descriptor takes two).
const GDT_ENTRIES: usize = 7;

static mut GDT: [u64; GDT_ENTRIES] = [0; GDT_ENTRIES];

#[repr(C, packed)]
struct DescriptorTablePointer {
    limit: u16,
    base: u64,
}

/// Builds the two-slot system descriptor for an available 64-bit TSS.
fn tss_descriptor(base: u64, limit: u32) -> (u64, u64) {
    let low = (limit as u64 & 0xFFFF)
        | ((base & 0x00FF_FFFF) << 16)
        | (0x89u64 << 40) // present, DPL 0, type = 64-bit TSS (available)
        | (((limit as u64 >> 16) & 0xF) << 48)
        | (((base >> 24) & 0xFF) << 56);
    let high = base >> 32;
    (low, high)
}

/// Builds, loads the GDT and TSS, and reloads all segment registers.
///
/// Must be called exactly once on the boot CPU, before the IDT is loaded.
pub fn init() {
    // SAFETY: Called once during single-threaded early boot with interrupts not yet
    // dispatching through our IDT. We have exclusive access to the statics, and the
    // descriptors written below follow the architectural encoding.
    unsafe {
        // IST stacks grow downward: point at the top of each region.
        let df_top = addr_of_mut!(DOUBLE_FAULT_STACK) as u64 + IST_STACK_SIZE as u64;
        let nmi_top = addr_of_mut!(NMI_STACK) as u64 + IST_STACK_SIZE as u64;
        let tss = addr_of_mut!(TSS);
        (*tss).ist[(DOUBLE_FAULT_IST - 1) as usize] = df_top;
        (*tss).ist[(NMI_IST - 1) as usize] = nmi_top;

        let (tss_low, tss_high) = tss_descriptor(tss as u64, (size_of::<Tss>() - 1) as u32);

        let gdt = addr_of_mut!(GDT);
        (*gdt)[0] = 0; // null
        (*gdt)[1] = 0x00AF_9A00_0000_FFFF; // kernel code: L=1, P=1, DPL0, exec/read
        (*gdt)[2] = 0x00CF_9200_0000_FFFF; // kernel data: P=1, DPL0, read/write
        (*gdt)[3] = 0x00CF_F200_0000_FFFF; // user data:   P=1, DPL3, read/write
        (*gdt)[4] = 0x00AF_FA00_0000_FFFF; // user code:   L=1, P=1, DPL3, exec/read
        (*gdt)[5] = tss_low;
        (*gdt)[6] = tss_high;

        let pointer = DescriptorTablePointer {
            limit: (size_of::<[u64; GDT_ENTRIES]>() - 1) as u16,
            base: gdt as u64,
        };

        asm!(
            "lgdt [{ptr}]",
            ptr = in(reg) &pointer,
            options(readonly, nostack, preserves_flags)
        );

        // Reload CS with a far return, then the data segment registers.
        asm!(
            "push {cs}",
            "lea {tmp}, [rip + 2f]",
            "push {tmp}",
            "retfq",
            "2:",
            cs = in(reg) KERNEL_CODE_SELECTOR as u64,
            tmp = lateout(reg) _,
            options(preserves_flags)
        );
        asm!(
            "mov ds, {sel:x}",
            "mov es, {sel:x}",
            "mov ss, {sel:x}",
            "mov fs, {null:x}",
            "mov gs, {null:x}",
            sel = in(reg) KERNEL_DATA_SELECTOR,
            null = in(reg) 0u16,
            options(nostack, preserves_flags)
        );

        asm!(
            "ltr {sel:x}",
            sel = in(reg) TSS_SELECTOR,
            options(nostack, preserves_flags)
        );
    }
}
