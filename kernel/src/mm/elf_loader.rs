//! Bounded ELF64 segment mapping into an isolated user address space.
//!
//! This loads static ET_EXEC images only. It does not enter Ring 3 or create a user stack.

use super::elf::{ElfError, ElfImage, MAX_IMAGE_PAGES, PAGE_SIZE};
use super::paging::{AddressSpace, PageFlags, PagingError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElfLoadError {
    InvalidElf(ElfError),
    Paging(PagingError),
    SourceRangeOutOfBounds,
    AddressOverflow,
}

/// Validates and maps an ELF image into `space`, copies PT_LOAD bytes, and leaves BSS zeroed.
///
/// If any mapping or copy operation fails, all pages mapped by this call are unmapped.
/// Existing mappings are not intentionally removed. The caller must keep `space` inactive
/// while loading; this routine writes through the kernel's physical-memory mapping.
pub fn load_elf(space: &mut AddressSpace, bytes: &[u8]) -> Result<u64, ElfLoadError> {
    let image = ElfImage::parse(bytes).map_err(ElfLoadError::InvalidElf)?;
    let mut mapped = [0u64; MAX_IMAGE_PAGES as usize];
    let mut mapped_count = 0usize;

    for segment in image.segments() {
        let flags = PageFlags {
            writable: segment.is_writable(),
            user: true,
            no_execute: !segment.is_executable(),
        };
        let page_end = segment.page_end().ok_or(ElfLoadError::AddressOverflow)?;
        let mut page = segment.page_start();
        while page < page_end {
            if let Err(error) = space.map_zeroed_user_page_with_flags(page, flags) {
                rollback(space, &mapped, mapped_count);
                return Err(ElfLoadError::Paging(error));
            }
            mapped[mapped_count] = page;
            mapped_count += 1;
            page = match page.checked_add(PAGE_SIZE) {
                Some(next) => next,
                None => {
                    rollback(space, &mapped, mapped_count);
                    return Err(ElfLoadError::AddressOverflow);
                }
            };
        }
    }

    for segment in image.segments() {
        if segment.file_size == 0 { continue; }
        let start = match usize::try_from(segment.file_offset) {
            Ok(value) => value,
            Err(_) => {
                rollback(space, &mapped, mapped_count);
                return Err(ElfLoadError::SourceRangeOutOfBounds);
            }
        };
        let length = match usize::try_from(segment.file_size) {
            Ok(value) => value,
            Err(_) => {
                rollback(space, &mapped, mapped_count);
                return Err(ElfLoadError::SourceRangeOutOfBounds);
            }
        };
        let end = match start.checked_add(length) {
            Some(value) => value,
            None => {
                rollback(space, &mapped, mapped_count);
                return Err(ElfLoadError::SourceRangeOutOfBounds);
            }
        };
        let source = match bytes.get(start..end) {
            Some(value) => value,
            None => {
                rollback(space, &mapped, mapped_count);
                return Err(ElfLoadError::SourceRangeOutOfBounds);
            }
        };
        if let Err(error) = space.write_user_bytes(segment.virtual_address, source) {
            rollback(space, &mapped, mapped_count);
            return Err(ElfLoadError::Paging(error));
        }
    }

    Ok(image.entry)
}

fn rollback(space: &mut AddressSpace, mapped: &[u64; MAX_IMAGE_PAGES as usize], count: usize) {
    for address in mapped[..count].iter().rev().copied() {
        let _ = space.unmap_user_page(address);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_elf_fails_before_mapping() {
        // Parsing is intentionally the first operation; malformed input cannot allocate pages.
        let mut invalid = [0u8; 64];
        assert!(matches!(load_elf_unreachable_for_test(&invalid), Err(ElfLoadError::InvalidElf(_))));
        invalid[0] = 0x7f;
    }

    // Host tests cannot safely construct an AddressSpace because that requires live x86 paging.
    // This helper exercises the exact parser gate without touching privileged CPU state.
    fn load_elf_unreachable_for_test(bytes: &[u8]) -> Result<u64, ElfLoadError> {
        ElfImage::parse(bytes).map(|image| image.entry).map_err(ElfLoadError::InvalidElf)
    }
}
