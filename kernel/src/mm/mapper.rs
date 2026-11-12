//! Architecture-neutral contracts for translating physical memory into kernel mappings.

use core::ptr::NonNull;

/// Maps a physical byte range into a writable kernel virtual address.
///
/// This interface deliberately contains no boot-protocol or architecture assumptions.
///
/// # Safety
/// Implementations must return a pointer that remains valid for the requested range for
/// the lifetime of the kernel. The mapping must be writable, contiguous, and correctly
/// aligned for any type the caller places there. The physical range must be backed by RAM.
pub unsafe trait PhysicalMemoryMapper {
    /// Resolve a physical range. A zero-sized range may return a non-null aligned pointer.
    ///
    /// # Safety
    /// The caller must uphold the mapper implementation's range and lifetime contract.
    unsafe fn map_physical(&self, physical_address: u64, size: usize) -> Option<NonNull<u8>>;
}
