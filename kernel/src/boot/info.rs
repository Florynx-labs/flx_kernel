//! Boot information analysis and memory map parsing.

use bootloader_api::info::{BootInfo, MemoryRegionKind};

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
