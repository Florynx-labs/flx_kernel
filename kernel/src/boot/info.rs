//! Boot information analysis and memory map parsing.

use bootloader_api::info::{BootInfo, MemoryRegion, MemoryRegionKind};
use core::ptr::NonNull;

use crate::mm::frame::{FrameMemoryRegion, PhysicalMemoryMapper};

/// Adapt the bootloader descriptor to the architecture-neutral frame allocator contract.
impl FrameMemoryRegion for MemoryRegion {
    fn start(&self) -> u64 { self.start }
    fn end(&self) -> u64 { self.end }
    fn is_usable(&self) -> bool { self.kind == MemoryRegionKind::Usable }
}

/// Adapter for the UEFI bootloader's dynamic linear physical-memory mapping.
///
/// Keep this boot-protocol detail outside the architecture-neutral memory manager.
pub struct LinearPhysicalMemoryMapper {
    virtual_offset: u64,
}

impl LinearPhysicalMemoryMapper {
    /// Creates an adapter for a bootloader-provided linear physical map.
    ///
    /// # Safety
    /// `virtual_offset` must identify a writable, contiguous mapping of every physical
    /// RAM range the frame allocator may request, and it must remain active permanently.
    pub unsafe fn new(virtual_offset: u64) -> Self {
        Self { virtual_offset }
    }
}

// SAFETY: construction is unsafe and requires the bootloader mapping contract documented
// above. The implementation performs checked address arithmetic for every requested range.
unsafe impl PhysicalMemoryMapper for LinearPhysicalMemoryMapper {
    unsafe fn map_physical(&self, physical_address: u64, size: usize) -> Option<NonNull<u8>> {
        let virtual_address = self.virtual_offset.checked_add(physical_address)?;
        if size > 0 {
            let last_byte = u64::try_from(size - 1).ok()?;
            virtual_address.checked_add(last_byte)?;
        }
        NonNull::new(virtual_address as *mut u8)
    }
}

/// High-level summary of system physical memory reported by UEFI/bootloader.
#[derive(Debug, Clone, Copy, Default)]
pub struct MemorySummary {
    pub total_bytes: u64,
    pub usable_bytes: u64,
    pub reserved_bytes: u64,
    pub bootloader_bytes: u64,
    pub region_count: usize,
}

/// Structured boot information parsed from the bootloader.
pub struct KernelBootInfo<'a> {
    raw: &'a BootInfo,
    pub memory_summary: MemorySummary,
}

impl<'a> KernelBootInfo<'a> {
    /// Inspects and parses raw boot information passed by the bootloader.
    pub fn parse(boot_info: &'a BootInfo) -> Self {
        let mut summary = MemorySummary::default();

        for region in boot_info.memory_regions.iter() {
            // Ignore malformed regions instead of allowing subtraction to
            // underflow. Saturating arithmetic keeps corrupt map entries from
            // wrapping the reported memory totals.
            let Some(size) = region.end.checked_sub(region.start) else {
                continue;
            };

            summary.region_count = summary.region_count.saturating_add(1);
            summary.total_bytes = summary.total_bytes.saturating_add(size);

            match region.kind {
                MemoryRegionKind::Usable => {
                    summary.usable_bytes = summary.usable_bytes.saturating_add(size);
                }
                MemoryRegionKind::Bootloader => {
                    summary.bootloader_bytes =
                        summary.bootloader_bytes.saturating_add(size);
                }
                _ => {
                    summary.reserved_bytes = summary.reserved_bytes.saturating_add(size);
                }
            }
        }

        Self {
            raw: boot_info,
            memory_summary: summary,
        }
    }

    /// Returns the physical memory mapping virtual offset, if present.
    pub fn physical_memory_offset(&self) -> Option<u64> {
        self.raw.physical_memory_offset.into_option()
    }

    /// Returns the ACPI RSDP table physical address, if discovered by UEFI.
    pub fn rsdp_addr(&self) -> Option<u64> {
        self.raw.rsdp_addr.into_option()
    }

    /// Returns an iterator over all memory regions.
    pub fn memory_regions(&self) -> impl Iterator<Item = &bootloader_api::info::MemoryRegion> {
        self.raw.memory_regions.iter()
    }
}
