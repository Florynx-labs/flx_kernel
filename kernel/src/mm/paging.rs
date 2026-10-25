//! x86_64 four-level page-table primitives for 4 KiB pages.
//!
//! The bootloader already enables paging. This module edits the active CR3 tree;
//! it never replaces CR3 or assumes that a fresh address space is safe to install.
//! Physical page-table frames are accessed through the bootloader's physical-memory map.

use crate::mm::frame::{self, FRAME_SIZE};

const ENTRY_COUNT: usize = 512;
const PRESENT: u64 = 1 << 0;
const WRITABLE: u64 = 1 << 1;
const USER: u64 = 1 << 2;
const HUGE_PAGE: u64 = 1 << 7;
const NO_EXECUTE: u64 = 1 << 63;
const ADDRESS_MASK: u64 = 0x000f_ffff_ffff_f000;
const CR3_ADDRESS_MASK: u64 = 0x000f_ffff_ffff_f000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageFlags {
    pub writable: bool,
    pub user: bool,
    /// Set NX to prevent instruction fetches from this page.
    pub no_execute: bool,
}

impl PageFlags {
    pub const KERNEL_DATA: Self = Self { writable: true, user: false, no_execute: true };
    pub const KERNEL_CODE: Self = Self { writable: false, user: false, no_execute: false };
    pub const USER_DATA: Self = Self { writable: true, user: true, no_execute: true };

    fn bits(self) -> u64 {
        PRESENT
            | if self.writable { WRITABLE } else { 0 }
            | if self.user { USER } else { 0 }
            | if self.no_execute { NO_EXECUTE } else { 0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PagingError {
    UnalignedAddress,
    NonCanonicalAddress,
    PhysicalAddressOutOfRange,
    AddressOverflow,
    OutOfFrames,
    AlreadyMapped,
    NotMapped,
    HugePageConflict,
    InvalidPageTable,
    NxNotEnabled,
}

/// Handle to the currently active four-level x86_64 page tables.
///
/// The caller must supply the physical-memory offset reported by the bootloader.
pub struct ActivePageTable {
    root_phys: u64,
    phys_offset: u64,
}

impl ActivePageTable {
    /// Opens the active page-table hierarchy without changing CR3.
    ///
    /// # Safety
    /// phys_offset must be a valid writable mapping for all physical addresses used
    /// by the active page tables and physical frame allocator.
    pub unsafe fn current(phys_offset: u64) -> Self {
        let cr3: u64;
        core::arch::asm!("mov {}, cr3", out(reg) cr3, options(nomem, nostack, preserves_flags));
        Self { root_phys: cr3 & CR3_ADDRESS_MASK, phys_offset }
    }

    /// Returns the physical address backing a virtual address, including the page offset.
    /// Supports 4 KiB, 2 MiB, and 1 GiB leaves.
    pub fn translate(&self, virtual_addr: u64) -> Result<Option<u64>, PagingError> {
        if !is_canonical(virtual_addr) {
            return Err(PagingError::NonCanonicalAddress);
        }

        let idx = indices(virtual_addr);
        let pml4 = unsafe { self.table(self.root_phys)? };
        let e4 = unsafe { read_volatile(&(*pml4)[idx[0]]) };
        if e4 & PRESENT == 0 { return Ok(None); }

        let pdpt = unsafe { self.table(e4 & ADDRESS_MASK)? };
        let e3 = unsafe { read_volatile(&(*pdpt)[idx[1]]) };
        if e3 & PRESENT == 0 { return Ok(None); }
        if e3 & HUGE_PAGE != 0 {
            return Ok(Some((e3 & 0x000f_ffff_c000_0000) | (virtual_addr & 0x3fff_ffff)));
        }

        let pd = unsafe { self.table(e3 & ADDRESS_MASK)? };
        let e2 = unsafe { read_volatile(&(*pd)[idx[2]]) };
        if e2 & PRESENT == 0 { return Ok(None); }
        if e2 & HUGE_PAGE != 0 {
            return Ok(Some((e2 & 0x000f_ffff_ffe0_0000) | (virtual_addr & 0x1f_ffff)));
        }

        let pt = unsafe { self.table(e2 & ADDRESS_MASK)? };
        let e1 = unsafe { read_volatile(&(*pt)[idx[3]]) };
        if e1 & PRESENT == 0 { return Ok(None); }
        Ok(Some((e1 & ADDRESS_MASK) | (virtual_addr & (FRAME_SIZE - 1))))
    }

    /// Maps one aligned 4 KiB virtual page to an aligned physical frame.
    /// Existing mappings are never overwritten; the caller retains ownership of the frame.
    pub fn map_4k(
        &mut self,
        virtual_addr: u64,
        physical_addr: u64,
        flags: PageFlags,
    ) -> Result<(), PagingError> {
        if virtual_addr % FRAME_SIZE != 0 || physical_addr % FRAME_SIZE != 0 {
            return Err(PagingError::UnalignedAddress);
        }
        if !is_canonical(virtual_addr) {
            return Err(PagingError::NonCanonicalAddress);
        }
        if physical_addr & !ADDRESS_MASK != 0 {
            return Err(PagingError::PhysicalAddressOutOfRange);
        }
        // Setting bit 63 in a PTE is reserved unless EFER.NXE is enabled.
        // Fail safely rather than turning an ordinary mapping into a reserved-bit fault.
        if flags.no_execute && !nx_enabled() {
            return Err(PagingError::NxNotEnabled);
        }

        let idx = indices(virtual_addr);
        let mut table_phys = self.root_phys;

        for level in 0..3 {
            let table = unsafe { self.table(table_phys)? };
            let entry = unsafe { &mut (*table)[idx[level]] };
            let current = unsafe { read_volatile(entry) };

            if current & PRESENT == 0 {
                let next = frame::alloc_frame().ok_or(PagingError::OutOfFrames)?;
                let next_ptr = self.phys_ptr(next)?;
                unsafe {
                    core::ptr::write_bytes(next_ptr, 0, FRAME_SIZE as usize);
                    write_volatile(entry, (next & ADDRESS_MASK) | PRESENT | WRITABLE | if flags.user { USER } else { 0 });
                }
                table_phys = next;
            } else {
                if (level == 1 || level == 2) && current & HUGE_PAGE != 0 {
                    return Err(PagingError::HugePageConflict);
                }
                if flags.user && current & USER == 0 {
                    unsafe { write_volatile(entry, current | USER | WRITABLE); }
                }
                table_phys = current & ADDRESS_MASK;
            }
        }

        let pt = unsafe { self.table(table_phys)? };
        let leaf = unsafe { &mut (*pt)[idx[3]] };
        if unsafe { read_volatile(leaf) } & PRESENT != 0 {
            return Err(PagingError::AlreadyMapped);
        }
        unsafe { write_volatile(leaf, (physical_addr & ADDRESS_MASK) | flags.bits()); }
        flush_tlb_page(virtual_addr);
        Ok(())
    }

    /// Maps a batch of physical frames into consecutive virtual pages.
    ///
    /// If any mapping fails, mappings created by this call are removed before returning
    /// the original error. Physical frames remain owned by the caller. Intermediate page
    /// tables created during a failed attempt are currently retained for later reuse.
    pub fn map_4k_range(
        &mut self,
        virtual_start: u64,
        physical_frames: &[u64],
        flags: PageFlags,
    ) -> Result<(), PagingError> {
        if virtual_start % FRAME_SIZE != 0 {
            return Err(PagingError::UnalignedAddress);
        }
        if !is_canonical(virtual_start) {
            return Err(PagingError::NonCanonicalAddress);
        }
        if physical_frames.is_empty() {
            return Ok(());
        }

        let span = u64::try_from(physical_frames.len())
            .ok()
            .and_then(|count| count.checked_mul(FRAME_SIZE))
            .ok_or(PagingError::AddressOverflow)?;
        let last = virtual_start
            .checked_add(span - FRAME_SIZE)
            .ok_or(PagingError::AddressOverflow)?;
        if !is_canonical(last) || ((virtual_start >> 47) & 1) != ((last >> 47) & 1) {
            return Err(PagingError::NonCanonicalAddress);
        }

        for (index, physical) in physical_frames.iter().copied().enumerate() {
            let offset = u64::try_from(index)
                .ok()
                .and_then(|i| i.checked_mul(FRAME_SIZE))
                .ok_or(PagingError::AddressOverflow)?;
            let virtual_addr = virtual_start
                .checked_add(offset)
                .ok_or(PagingError::AddressOverflow)?;
            if let Err(error) = self.map_4k(virtual_addr, physical, flags) {
                for rollback_index in 0..index {
                    let rollback_offset = (rollback_index as u64) * FRAME_SIZE;
                    // Best-effort rollback: preserve the mapping error that caused failure.
                    let _ = self.unmap_4k(virtual_start + rollback_offset);
                }
                return Err(error);
            }
        }
        Ok(())
    }

    /// Removes a 4 KiB mapping and returns its physical frame; does not free that frame.
    pub fn unmap_4k(&mut self, virtual_addr: u64) -> Result<u64, PagingError> {
        if virtual_addr % FRAME_SIZE != 0 {
            return Err(PagingError::UnalignedAddress);
        }
        if !is_canonical(virtual_addr) {
            return Err(PagingError::NonCanonicalAddress);
        }

        let idx = indices(virtual_addr);
        let mut table_phys = self.root_phys;
        for level in 0..3 {
            let table = unsafe { self.table(table_phys)? };
            let entry = unsafe { read_volatile(&(*table)[idx[level]]) };
            if entry & PRESENT == 0 { return Err(PagingError::NotMapped); }
            if (level == 1 || level == 2) && entry & HUGE_PAGE != 0 {
                return Err(PagingError::HugePageConflict);
            }
            table_phys = entry & ADDRESS_MASK;
        }

        let pt = unsafe { self.table(table_phys)? };
        let leaf = unsafe { &mut (*pt)[idx[3]] };
        let old = unsafe { read_volatile(leaf) };
        if old & PRESENT == 0 { return Err(PagingError::NotMapped); }
        unsafe { write_volatile(leaf, 0); }
        flush_tlb_page(virtual_addr);
        Ok(old & ADDRESS_MASK)
    }

    unsafe fn table(&self, physical_addr: u64) -> Result<*mut [u64; ENTRY_COUNT], PagingError> {
        if physical_addr & (FRAME_SIZE - 1) != 0 {
            return Err(PagingError::InvalidPageTable);
        }
        Ok(self.phys_ptr(physical_addr)?.cast::<[u64; ENTRY_COUNT]>())
    }

    fn phys_ptr(&self, physical_addr: u64) -> Result<*mut u8, PagingError> {
        let virtual_addr = self.phys_offset.checked_add(physical_addr).ok_or(PagingError::AddressOverflow)?;
        if !is_canonical(virtual_addr) {
            return Err(PagingError::NonCanonicalAddress);
        }
        Ok(virtual_addr as *mut u8)
    }
}

/// Returns whether the processor has NX support and the kernel has enabled EFER.NXE.
#[inline]
fn nx_enabled() -> bool {
    let max_extended: u32;
    unsafe {
        core::arch::asm!(
            "push rbx",
            "cpuid",
            "pop rbx",
            inout("eax") 0x8000_0000u32 => max_extended,
            lateout("ecx") _,
            lateout("edx") _,
            options(preserves_flags)
        );
    }
    if max_extended < 0x8000_0001 {
        return false;
    }

    let extended_features_edx: u32;
    unsafe {
        core::arch::asm!(
            "push rbx",
            "cpuid",
            "pop rbx",
            inout("eax") 0x8000_0001u32 => _,
            lateout("ecx") _,
            lateout("edx") extended_features_edx,
            options(preserves_flags)
        );
    }
    const CPUID_NX: u32 = 1 << 20;
    if extended_features_edx & CPUID_NX == 0 {
        return false;
    }

    let efer_low: u32;
    let efer_high: u32;
    unsafe {
        core::arch::asm!(
            "rdmsr",
            in("ecx") 0xC000_0080u32,
            out("eax") efer_low,
            out("edx") efer_high,
            options(nomem, nostack, preserves_flags)
        );
    }
    let efer = ((efer_high as u64) << 32) | efer_low as u64;
    efer & (1 << 11) != 0
}

#[inline]
fn indices(address: u64) -> [usize; 4] {
    [
        ((address >> 39) & 0x1ff) as usize,
        ((address >> 30) & 0x1ff) as usize,
        ((address >> 21) & 0x1ff) as usize,
        ((address >> 12) & 0x1ff) as usize,
    ]
}

#[inline]
fn is_canonical(address: u64) -> bool {
    let upper = address >> 48;
    let sign = (address >> 47) & 1;
    if sign == 0 { upper == 0 } else { upper == 0xffff }
}

#[inline]
unsafe fn read_volatile<T: Copy>(ptr: *const T) -> T {
    core::ptr::read_volatile(ptr)
}

#[inline]
unsafe fn write_volatile<T>(ptr: *mut T, value: T) {
    core::ptr::write_volatile(ptr, value);
}

#[inline]
fn flush_tlb_page(address: u64) {
    unsafe { core::arch::asm!("invlpg [{}]", in(reg) address, options(nostack, preserves_flags)); }
}

#[cfg(test)]
mod tests {
    use super::{indices, is_canonical};

    #[test]
    fn splits_virtual_address_into_four_indices() {
        assert_eq!(indices(0), [0, 0, 0, 0]);
        assert_eq!(indices(0x0000_7fff_ffff_f000), [255, 511, 511, 511]);
        assert_eq!(indices(0xffff_8000_0000_0000), [256, 0, 0, 0]);
    }

    #[test]
    fn checks_48_bit_canonical_addresses() {
        assert!(is_canonical(0x0000_7fff_ffff_ffff));
        assert!(is_canonical(0xffff_8000_0000_0000));
        assert!(!is_canonical(0x0000_8000_0000_0000));
        assert!(!is_canonical(0xffff_7fff_ffff_ffff));
    }
}
