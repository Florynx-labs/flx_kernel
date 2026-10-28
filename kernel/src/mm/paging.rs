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
    pub const USER_CODE: Self = Self { writable: false, user: true, no_execute: false };

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
                // Intermediate entries participate in effective permissions. A writable or
                // user leaf cannot override restrictive parent bits, and a parent NX bit
                // would make an executable leaf unusable. Relax only the parent bits required
                // by this mapping; the leaf continues to enforce its own permissions.
                let updated = merge_parent_flags(current, flags);
                if updated != current {
                    unsafe { write_volatile(entry, updated); }
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

        // Validate every physical frame before changing any page-table entry.
        // This keeps malformed batches side-effect-free (no map/rollback cycle needed).
        for physical in physical_frames.iter().copied() {
            if physical % FRAME_SIZE != 0 {
                return Err(PagingError::UnalignedAddress);
            }
            if physical & !ADDRESS_MASK != 0 {
                return Err(PagingError::PhysicalAddressOutOfRange);
            }
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

/// Maximum number of simultaneously mapped user pages tracked by one address space.
pub const MAX_USER_PAGES: usize = 256;
const USER_MIN_ADDRESS: u64 = 0x1_0000;
const USER_ADDRESS_LIMIT: u64 = 0x0000_8000_0000_0000;

/// Independent x86_64 page-table root with shared supervisor mappings in the upper half.
///
/// User mappings are created only through this type, which allocates and owns each data frame.
/// The upper PML4 half is copied from the active kernel page tables so kernel code, stack,
/// physical-memory mapping, and device mappings remain available after CR3 switches.
pub struct AddressSpace {
    root_phys: u64,
    phys_offset: u64,
    mapped_pages: [u64; MAX_USER_PAGES],
    mapped_count: usize,
}

impl AddressSpace {
    /// Creates a fresh address space with an empty user half and shared kernel mappings.
    ///
    /// # Safety
    /// phys_offset must be the valid writable physical-memory mapping supplied by the bootloader.
    pub unsafe fn new(phys_offset: u64) -> Result<Self, PagingError> {
        // SAFETY: forwarded contract guarantees the physical-memory mapping is valid.
        let active = unsafe { ActivePageTable::current(phys_offset) };
        let root_phys = frame::alloc_frame().ok_or(PagingError::OutOfFrames)?;
        let root_ptr = match active.phys_ptr(root_phys) {
            Ok(pointer) => pointer.cast::<[u64; ENTRY_COUNT]>(),
            Err(error) => {
                let _ = frame::free_frame(root_phys);
                return Err(error);
            }
        };
        // SAFETY: root_phys is a newly allocated frame mapped by phys_offset.
        unsafe { core::ptr::write_bytes(root_ptr.cast::<u8>(), 0, FRAME_SIZE as usize); }

        let source = match unsafe { active.table(active.root_phys) } {
            Ok(table) => table,
            Err(error) => {
                let _ = frame::free_frame(root_phys);
                return Err(error);
            }
        };
        // Keep the upper-half supervisor mappings (including the physical-memory map) and
        // the kernel image's lower-half PML4 slot. This kernel is linked at 0x10000000000,
        // which is PML4 index 2. Do not copy other lower-half slots: user code (index 0) and
        // the user stack (index 255) need private page-table trees, not shared bootloader tables.
        // Clear USER on inherited root entries so kernel mappings remain supervisor-only.
        for index in 0..ENTRY_COUNT {
            if index < ENTRY_COUNT / 2 && index != 2 {
                continue;
            }
            let entry = unsafe { read_volatile(&(*source)[index]) };
            if entry & PRESENT != 0 {
                unsafe { write_volatile(&mut (*root_ptr)[index], entry & !USER); }
            }
        }

        Ok(Self {
            root_phys,
            phys_offset,
            mapped_pages: [0; MAX_USER_PAGES],
            mapped_count: 0,
        })
    }

    /// Ensures a supervisor-only kernel virtual page is present in this address space.
    ///
    /// Resolve the physical frame through the active boot/kernel page tables before CR3 is
    /// switched. This protects statically allocated kernel stacks across address-space changes.
    pub fn ensure_kernel_mapping(&mut self, virtual_addr: u64) -> Result<(), PagingError> {
        if virtual_addr % FRAME_SIZE != 0 {
            return Err(PagingError::UnalignedAddress);
        }
        if !is_canonical(virtual_addr) {
            return Err(PagingError::NonCanonicalAddress);
        }

        // SAFETY: called while the boot/kernel address space is active and phys_offset is valid.
        let active = unsafe { ActivePageTable::current(self.phys_offset) };
        let physical = active.translate(virtual_addr)?.ok_or(PagingError::NotMapped)?;
        let mut target = ActivePageTable {
            root_phys: self.root_phys,
            phys_offset: self.phys_offset,
        };
        match target.translate(virtual_addr)? {
            Some(existing) if existing == (physical & !(FRAME_SIZE - 1)) => Ok(()),
            Some(_) => Err(PagingError::AlreadyMapped),
            None => target.map_4k(
                virtual_addr,
                physical & !(FRAME_SIZE - 1),
                PageFlags::KERNEL_DATA,
            ),
        }
    }

    pub fn root_physical_address(&self) -> u64 {
        self.root_phys
    }

    pub fn mapped_user_pages(&self) -> usize {
        self.mapped_count
    }

    /// Allocates a zero-filled physical frame mapped as writable, non-executable user data.
    pub fn map_zeroed_user_page(&mut self, virtual_addr: u64) -> Result<(), PagingError> {
        self.map_zeroed_user_page_with_flags(virtual_addr, PageFlags::USER_DATA)
    }

    /// Allocates a zero-filled physical frame mapped as read-only, executable user code.
    pub fn map_zeroed_user_code_page(&mut self, virtual_addr: u64) -> Result<(), PagingError> {
        self.map_zeroed_user_page_with_flags(virtual_addr, PageFlags::USER_CODE)
    }

    pub fn map_zeroed_user_page_with_flags(
        &mut self,
        virtual_addr: u64,
        flags: PageFlags,
    ) -> Result<(), PagingError> {
        validate_user_page_address(virtual_addr)?;
        if !flags.user || (flags.writable && !flags.no_execute) {
            return Err(PagingError::InvalidPageTable);
        }
        if self.mapped_count == MAX_USER_PAGES {
            return Err(PagingError::OutOfFrames);
        }
        if self.mapped_pages[..self.mapped_count].contains(&virtual_addr) {
            return Err(PagingError::AlreadyMapped);
        }

        // SAFETY: phys_offset is validated by AddressSpace::new and remains valid.
        let mut table = ActivePageTable { root_phys: self.root_phys, phys_offset: self.phys_offset };
        if table.translate(virtual_addr)?.is_some() {
            return Err(PagingError::AlreadyMapped);
        }

        let physical = frame::alloc_frame().ok_or(PagingError::OutOfFrames)?;
        let physical_ptr = match table.phys_ptr(physical) {
            Ok(pointer) => pointer,
            Err(error) => {
                let _ = frame::free_frame(physical);
                return Err(error);
            }
        };
        // SAFETY: physical is a newly allocated, exclusively owned frame.
        unsafe { core::ptr::write_bytes(physical_ptr, 0, FRAME_SIZE as usize); }

        if let Err(error) = table.map_4k(virtual_addr, physical, flags) {
            let _ = frame::free_frame(physical);
            return Err(error);
        }
        self.mapped_pages[self.mapped_count] = virtual_addr;
        self.mapped_count += 1;
        Ok(())
    }

    /// Removes a user page and returns its data frame to the physical allocator.
    pub fn unmap_user_page(&mut self, virtual_addr: u64) -> Result<(), PagingError> {
        validate_user_page_address(virtual_addr)?;
        let Some(index) = self.mapped_pages[..self.mapped_count]
            .iter()
            .position(|address| *address == virtual_addr)
        else {
            return Err(PagingError::NotMapped);
        };

        let mut table = ActivePageTable { root_phys: self.root_phys, phys_offset: self.phys_offset };
        let physical = table.unmap_4k(virtual_addr)?;
        frame::free_frame(physical).map_err(|_| PagingError::InvalidPageTable)?;
        self.mapped_count -= 1;
        self.mapped_pages[index] = self.mapped_pages[self.mapped_count];
        self.mapped_pages[self.mapped_count] = 0;
        Ok(())
    }

    /// Copies bytes into already-mapped user pages through the physical-memory mapping.
    /// This deliberately bypasses user page write permissions only from kernel context.
    pub fn write_user_bytes(&mut self, virtual_addr: u64, source: &[u8]) -> Result<(), PagingError> {
        if source.is_empty() { return Ok(()); }
        let end = virtual_addr.checked_add(source.len() as u64).ok_or(PagingError::AddressOverflow)?;
        if virtual_addr < USER_MIN_ADDRESS || end > USER_ADDRESS_LIMIT || end <= virtual_addr {
            return Err(PagingError::NonCanonicalAddress);
        }
        let table = ActivePageTable { root_phys: self.root_phys, phys_offset: self.phys_offset };
        let mut copied = 0usize;
        while copied < source.len() {
            let current = virtual_addr.checked_add(copied as u64).ok_or(PagingError::AddressOverflow)?;
            let page = current & !(FRAME_SIZE - 1);
            if !self.mapped_pages[..self.mapped_count].contains(&page) {
                return Err(PagingError::NotMapped);
            }
            let physical = table.translate(current)?.ok_or(PagingError::NotMapped)?;
            let physical_page_remaining = (FRAME_SIZE - (physical & (FRAME_SIZE - 1))) as usize;
            let count = core::cmp::min(physical_page_remaining, source.len() - copied);
            let destination = table.phys_ptr(physical)?;
            // SAFETY: destination is a mapped physical frame owned by this address space;
            // source and destination are valid for count bytes and may not overlap.
            unsafe { core::ptr::copy_nonoverlapping(source.as_ptr().add(copied), destination, count); }
            copied += count;
        }
        Ok(())
    }

    /// Validates that every page touched by a user range belongs to this address space.
    ///
    /// Empty ranges are accepted without dereferencing the address. Non-empty ranges must stay
    /// entirely within the canonical lower-half user range and every page must be mapped by this
    /// AddressSpace instance.
    pub fn validate_user_range(&self, virtual_addr: u64, length: usize) -> Result<(), PagingError> {
        if length == 0 {
            return Ok(());
        }
        let length_u64 = u64::try_from(length).map_err(|_| PagingError::AddressOverflow)?;
        let end = virtual_addr.checked_add(length_u64).ok_or(PagingError::AddressOverflow)?;
        if virtual_addr < USER_MIN_ADDRESS || end > USER_ADDRESS_LIMIT || end <= virtual_addr {
            return Err(PagingError::NonCanonicalAddress);
        }

        let first_page = virtual_addr & !(FRAME_SIZE - 1);
        let last_page = (end - 1) & !(FRAME_SIZE - 1);
        let mut page = first_page;
        loop {
            if !self.mapped_pages[..self.mapped_count].contains(&page) {
                return Err(PagingError::NotMapped);
            }
            if page == last_page {
                break;
            }
            page = page.checked_add(FRAME_SIZE).ok_or(PagingError::AddressOverflow)?;
        }
        Ok(())
    }

    /// Copies bytes from mapped user memory into a kernel-owned destination buffer.
    ///
    /// This reads through the physical-memory mapping, so a user-controlled pointer is never
    /// directly dereferenced by the kernel. The destination must be a valid writable kernel slice.
    pub fn read_user_bytes(
        &self,
        virtual_addr: u64,
        destination: &mut [u8],
    ) -> Result<(), PagingError> {
        if destination.is_empty() {
            return Ok(());
        }
        self.validate_user_range(virtual_addr, destination.len())?;
        let table = ActivePageTable { root_phys: self.root_phys, phys_offset: self.phys_offset };
        let mut copied = 0usize;
        while copied < destination.len() {
            let current = virtual_addr
                .checked_add(copied as u64)
                .ok_or(PagingError::AddressOverflow)?;
            let physical = table.translate(current)?.ok_or(PagingError::NotMapped)?;
            let remaining_in_page = (FRAME_SIZE - (physical & (FRAME_SIZE - 1))) as usize;
            let count = core::cmp::min(remaining_in_page, destination.len() - copied);
            let source = table.phys_ptr(physical)?;
            // SAFETY: validate_user_range checked ownership of every user page, translation
            // resolves through this address space, and destination is a valid mutable slice.
            // copy (not copy_nonoverlapping) also tolerates an unusual aliasing destination.
            unsafe {
                core::ptr::copy(source, destination.as_mut_ptr().add(copied), count);
            }
            copied += count;
        }
        Ok(())
    }

    /// Switches CR3 to this address space.
    ///
    /// # Safety
    /// Caller must ensure kernel execution state is valid under the shared upper-half mappings,
    /// and must not switch away while holding locks whose data could be unmapped in this space.
    pub unsafe fn activate(&self) {
        // SAFETY: the root frame is page-aligned and populated by AddressSpace::new.
        unsafe {
            core::arch::asm!(
                "mov cr3, {}",
                in(reg) self.root_phys,
                options(nostack, preserves_flags)
            );
        }
    }

    /// Destroys this address space and frees its user data frames and private page-table frames.
    ///
    /// # Safety
    /// This address space must not be active on any CPU. No pointers into its user mappings may
    /// be in use. Kernel upper-half tables are shared and are never traversed or freed.
    pub unsafe fn destroy(self) -> Result<(), PagingError> {
        // SAFETY: the caller guarantees this root is inactive and no user mapping is in use.
        unsafe { free_user_subtree(self.root_phys, self.phys_offset)?; }
        frame::free_frame(self.root_phys).map_err(|_| PagingError::InvalidPageTable)?;
        Ok(())
    }
}

fn validate_user_page_address(address: u64) -> Result<(), PagingError> {
    if address % FRAME_SIZE != 0 {
        return Err(PagingError::UnalignedAddress);
    }
    if address < USER_MIN_ADDRESS || address >= USER_ADDRESS_LIMIT || !is_canonical(address) {
        return Err(PagingError::NonCanonicalAddress);
    }
    Ok(())
}

/// Recursively releases a private user-half page-table subtree (including its mapped data pages).
unsafe fn free_user_subtree(table_phys: u64, phys_offset: u64) -> Result<(), PagingError> {
    // SAFETY: callers pass page-table frames allocated for this address space.
    let table_ptr = phys_offset.checked_add(table_phys).ok_or(PagingError::AddressOverflow)? as *mut [u64; ENTRY_COUNT];
    for index in 0..(ENTRY_COUNT / 2) {
        let entry = unsafe { read_volatile(&(*table_ptr)[index]) };
        if entry & PRESENT == 0 {
            continue;
        }
        if entry & HUGE_PAGE != 0 {
            return Err(PagingError::HugePageConflict);
        }
        let child_phys = entry & ADDRESS_MASK;
        // At the leaf level the entry references user data, not another page table.
        // This helper is called for the PML4 root; infer level through the recursive helper.
        unsafe { free_user_table_level(child_phys, phys_offset, 3)?; }
        unsafe { write_volatile(&mut (*table_ptr)[index], 0); }
    }
    Ok(())
}

unsafe fn free_user_table_level(
    table_phys: u64,
    phys_offset: u64,
    level: usize,
) -> Result<(), PagingError> {
    let virtual_addr = phys_offset.checked_add(table_phys).ok_or(PagingError::AddressOverflow)?;
    if !is_canonical(virtual_addr) {
        return Err(PagingError::NonCanonicalAddress);
    }
    let table_ptr = virtual_addr as *mut [u64; ENTRY_COUNT];
    for index in 0..ENTRY_COUNT {
        let entry = unsafe { read_volatile(&(*table_ptr)[index]) };
        if entry & PRESENT == 0 {
            continue;
        }
        if entry & HUGE_PAGE != 0 {
            return Err(PagingError::HugePageConflict);
        }
        let child_phys = entry & ADDRESS_MASK;
        if level == 1 {
            frame::free_frame(child_phys).map_err(|_| PagingError::InvalidPageTable)?;
        } else {
            unsafe { free_user_table_level(child_phys, phys_offset, level - 1)?; }
            frame::free_frame(child_phys).map_err(|_| PagingError::InvalidPageTable)?;
        }
        unsafe { write_volatile(&mut (*table_ptr)[index], 0); }
    }
    Ok(())
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
fn merge_parent_flags(entry: u64, flags: PageFlags) -> u64 {
    let mut updated = entry;
    if flags.writable {
        updated |= WRITABLE;
    }
    if flags.user {
        updated |= USER;
    }
    if !flags.no_execute {
        updated &= !NO_EXECUTE;
    }
    updated
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
    use super::{indices, is_canonical, merge_parent_flags, PageFlags, NO_EXECUTE, USER, WRITABLE};

    #[test]
    fn splits_virtual_address_into_four_indices() {
        assert_eq!(indices(0), [0, 0, 0, 0]);
        assert_eq!(indices(0x0000_7fff_ffff_f000), [255, 511, 511, 511]);
        assert_eq!(indices(0xffff_8000_0000_0000), [256, 0, 0, 0]);
    }

    #[test]
    fn parent_permissions_allow_requested_leaf_access() {
        let restrictive = NO_EXECUTE;
        let flags = PageFlags { writable: true, user: true, no_execute: false };
        let updated = merge_parent_flags(restrictive, flags);
        assert_ne!(updated & WRITABLE, 0);
        assert_ne!(updated & USER, 0);
        assert_eq!(updated & NO_EXECUTE, 0);
    }

    #[test]
    fn data_mapping_does_not_weaken_execute_permission() {
        let flags = PageFlags::KERNEL_DATA;
        let updated = merge_parent_flags(WRITABLE | NO_EXECUTE, flags);
        assert_ne!(updated & NO_EXECUTE, 0);
    }

    #[test]
    fn validates_user_page_address_range_and_alignment() {
        use super::{validate_user_page_address, PagingError};
        assert_eq!(validate_user_page_address(0x4000_0000), Ok(()));
        assert_eq!(validate_user_page_address(0x4000_0001), Err(PagingError::UnalignedAddress));
        assert_eq!(validate_user_page_address(0x1000), Err(PagingError::NonCanonicalAddress));
        assert_eq!(validate_user_page_address(0x0000_8000_0000_0000), Err(PagingError::NonCanonicalAddress));
    }

    #[test]
    fn checks_48_bit_canonical_addresses() {
        assert!(is_canonical(0x0000_7fff_ffff_ffff));
        assert!(is_canonical(0xffff_8000_0000_0000));
        assert!(!is_canonical(0x0000_8000_0000_0000));
        assert!(!is_canonical(0xffff_7fff_ffff_ffff));
    }
}
