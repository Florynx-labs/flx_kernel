//! Bounded ELF64 validation for static x86_64 user executables.
//!
//! Accepts only little-endian ELF64 ET_EXEC images with bounded PT_LOAD segments.
//! Dynamic linking, interpreters, TLS, and writable-executable segments are rejected.

pub const ELF_HEADER_SIZE: usize = 64;
pub const PROGRAM_HEADER_SIZE: usize = 56;
pub const MAX_LOAD_SEGMENTS: usize = 16;
pub const MAX_IMAGE_PAGES: u64 = 256;
pub const PAGE_SIZE: u64 = 4096;
const USER_MIN_ADDRESS: u64 = 0x1_0000;
const USER_ADDRESS_LIMIT: u64 = 0x0000_8000_0000_0000;

const ET_EXEC: u16 = 2;
const EM_X86_64: u16 = 62;
const PT_LOAD: u32 = 1;
const PT_DYNAMIC: u32 = 2;
const PT_INTERP: u32 = 3;
const PT_TLS: u32 = 7;
const PF_X: u32 = 1;
const PF_W: u32 = 2;
const PF_R: u32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElfError {
    TruncatedHeader, BadMagic, UnsupportedClass, UnsupportedEndian, UnsupportedVersion,
    UnsupportedType, UnsupportedMachine, InvalidHeaderSize, InvalidProgramHeaderSize,
    EmptyProgramHeaderTable, TooManyProgramHeaders, ProgramHeaderTableOutOfBounds,
    UnsupportedProgramHeader, NoLoadableSegments, TooManyLoadSegments, InvalidSegment,
    SegmentOutOfBounds, InvalidAlignment, OverlappingSegments, WritableExecutableSegment,
    EntryNotExecutable, ImageTooLarge,
}

/// Validated PT_LOAD metadata. Bytes are copied to virtual_address; the remaining memory_size
/// bytes must be zero-filled by the address-space loader.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadSegment {
    pub file_offset: u64,
    pub virtual_address: u64,
    pub file_size: u64,
    pub memory_size: u64,
    pub flags: u32,
    pub alignment: u64,
}

impl LoadSegment {
    pub fn is_executable(&self) -> bool { self.flags & PF_X != 0 }
    pub fn is_writable(&self) -> bool { self.flags & PF_W != 0 }
    pub fn is_readable(&self) -> bool { self.flags & PF_R != 0 }
    pub fn page_start(&self) -> u64 { self.virtual_address & !(PAGE_SIZE - 1) }
    pub fn page_end(&self) -> Option<u64> {
        self.virtual_address.checked_add(self.memory_size)?
            .checked_add(PAGE_SIZE - 1).map(|value| value & !(PAGE_SIZE - 1))
    }
    pub fn page_count(&self) -> Option<u64> {
        self.page_end()?.checked_sub(self.page_start()).map(|bytes| bytes / PAGE_SIZE)
    }
}

/// Parsed image metadata; no allocations or references into untrusted bytes are retained.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ElfImage {
    pub entry: u64,
    segments: [LoadSegment; MAX_LOAD_SEGMENTS],
    segment_count: usize,
    total_pages: u64,
}

impl ElfImage {
    pub fn segments(&self) -> &[LoadSegment] { &self.segments[..self.segment_count] }
    pub fn total_pages(&self) -> u64 { self.total_pages }

    /// Validate a static x86_64 ELF executable before allocating user memory.
    pub fn parse(bytes: &[u8]) -> Result<Self, ElfError> {
        if bytes.len() < ELF_HEADER_SIZE { return Err(ElfError::TruncatedHeader); }
        if &bytes[0..4] != b"\x7fELF" { return Err(ElfError::BadMagic); }
        if bytes[4] != 2 { return Err(ElfError::UnsupportedClass); }
        if bytes[5] != 1 { return Err(ElfError::UnsupportedEndian); }
        if bytes[6] != 1 { return Err(ElfError::UnsupportedVersion); }

        let kind = read_u16(bytes, 16)?;
        let machine = read_u16(bytes, 18)?;
        let version = read_u32(bytes, 20)?;
        let entry = read_u64(bytes, 24)?;
        let phoff = read_u64(bytes, 32)?;
        let ehsize = read_u16(bytes, 52)? as usize;
        let phentsize = read_u16(bytes, 54)? as usize;
        let phnum = read_u16(bytes, 56)? as usize;

        if kind != ET_EXEC { return Err(ElfError::UnsupportedType); }
        if machine != EM_X86_64 { return Err(ElfError::UnsupportedMachine); }
        if version != 1 { return Err(ElfError::UnsupportedVersion); }
        if ehsize != ELF_HEADER_SIZE { return Err(ElfError::InvalidHeaderSize); }
        if phentsize != PROGRAM_HEADER_SIZE { return Err(ElfError::InvalidProgramHeaderSize); }
        if phnum == 0 { return Err(ElfError::EmptyProgramHeaderTable); }
        if phnum > MAX_LOAD_SEGMENTS { return Err(ElfError::TooManyProgramHeaders); }

        let table_size = phnum.checked_mul(phentsize).ok_or(ElfError::ProgramHeaderTableOutOfBounds)?;
        let table_start = usize::try_from(phoff).map_err(|_| ElfError::ProgramHeaderTableOutOfBounds)?;
        let table_end = table_start.checked_add(table_size).ok_or(ElfError::ProgramHeaderTableOutOfBounds)?;
        if table_end > bytes.len() { return Err(ElfError::ProgramHeaderTableOutOfBounds); }

        let empty = LoadSegment {
            file_offset: 0, virtual_address: 0, file_size: 0, memory_size: 0,
            flags: 0, alignment: 0,
        };
        let mut segments = [empty; MAX_LOAD_SEGMENTS];
        let mut count = 0usize;
        let mut total_pages = 0u64;

        for index in 0..phnum {
            let offset = table_start + index * phentsize;
            let kind = read_u32(bytes, offset)?;
            if kind == PT_DYNAMIC || kind == PT_INTERP || kind == PT_TLS {
                return Err(ElfError::UnsupportedProgramHeader);
            }
            if kind != PT_LOAD { continue; }
            if count == MAX_LOAD_SEGMENTS { return Err(ElfError::TooManyLoadSegments); }

            let flags = read_u32(bytes, offset + 4)?;
            let file_offset = read_u64(bytes, offset + 8)?;
            let virtual_address = read_u64(bytes, offset + 16)?;
            let file_size = read_u64(bytes, offset + 32)?;
            let memory_size = read_u64(bytes, offset + 40)?;
            let alignment = read_u64(bytes, offset + 48)?;

            if memory_size == 0 || file_size > memory_size || flags & !(PF_R | PF_W | PF_X) != 0 {
                return Err(ElfError::InvalidSegment);
            }
            if flags & PF_R == 0 { return Err(ElfError::InvalidSegment); }
            if flags & (PF_W | PF_X) == (PF_W | PF_X) {
                return Err(ElfError::WritableExecutableSegment);
            }
            let file_end = file_offset.checked_add(file_size).ok_or(ElfError::SegmentOutOfBounds)?;
            if file_end > bytes.len() as u64 { return Err(ElfError::SegmentOutOfBounds); }
            let memory_end = virtual_address.checked_add(memory_size).ok_or(ElfError::SegmentOutOfBounds)?;
            if virtual_address < USER_MIN_ADDRESS || memory_end > USER_ADDRESS_LIMIT || memory_end <= virtual_address {
                return Err(ElfError::SegmentOutOfBounds);
            }
            if alignment > 1 && (!alignment.is_power_of_two() || file_offset % alignment != virtual_address % alignment) {
                return Err(ElfError::InvalidAlignment);
            }
            if file_offset % PAGE_SIZE != virtual_address % PAGE_SIZE {
                return Err(ElfError::InvalidAlignment);
            }

            let segment = LoadSegment { file_offset, virtual_address, file_size, memory_size, flags, alignment };
            let pages = segment.page_count().ok_or(ElfError::SegmentOutOfBounds)?;
            total_pages = total_pages.checked_add(pages).ok_or(ElfError::ImageTooLarge)?;
            if total_pages > MAX_IMAGE_PAGES { return Err(ElfError::ImageTooLarge); }

            for previous in &segments[..count] {
                let previous_end = previous.virtual_address.checked_add(previous.memory_size)
                    .ok_or(ElfError::SegmentOutOfBounds)?;
                if virtual_address < previous_end && previous.virtual_address < memory_end {
                    return Err(ElfError::OverlappingSegments);
                }
                let previous_page_end = previous.page_end().ok_or(ElfError::SegmentOutOfBounds)?;
                let page_end = segment.page_end().ok_or(ElfError::SegmentOutOfBounds)?;
                if segment.page_start() < previous_page_end && previous.page_start() < page_end {
                    return Err(ElfError::OverlappingSegments);
                }
            }
            segments[count] = segment;
            count += 1;
        }

        if count == 0 { return Err(ElfError::NoLoadableSegments); }
        let entry_is_executable = segments[..count].iter().any(|segment| {
            segment.is_executable() && entry >= segment.virtual_address
                && segment.virtual_address.checked_add(segment.memory_size).is_some_and(|end| entry < end)
        });
        if !entry_is_executable { return Err(ElfError::EntryNotExecutable); }
        Ok(Self { entry, segments, segment_count: count, total_pages })
    }
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, ElfError> {
    let end = offset.checked_add(2).ok_or(ElfError::TruncatedHeader)?;
    let slice = bytes.get(offset..end).ok_or(ElfError::TruncatedHeader)?;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}
fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, ElfError> {
    let end = offset.checked_add(4).ok_or(ElfError::TruncatedHeader)?;
    let slice = bytes.get(offset..end).ok_or(ElfError::TruncatedHeader)?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}
fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, ElfError> {
    let end = offset.checked_add(8).ok_or(ElfError::TruncatedHeader)?;
    let slice = bytes.get(offset..end).ok_or(ElfError::TruncatedHeader)?;
    Ok(u64::from_le_bytes([slice[0], slice[1], slice[2], slice[3], slice[4], slice[5], slice[6], slice[7]]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_elf(flags: u32) -> [u8; ELF_HEADER_SIZE + PROGRAM_HEADER_SIZE] {
        let mut bytes = [0u8; ELF_HEADER_SIZE + PROGRAM_HEADER_SIZE];
        bytes[0..4].copy_from_slice(b"\x7fELF");
        bytes[4] = 2;
        bytes[5] = 1;
        bytes[6] = 1;
        bytes[16..18].copy_from_slice(&ET_EXEC.to_le_bytes());
        bytes[18..20].copy_from_slice(&EM_X86_64.to_le_bytes());
        bytes[20..24].copy_from_slice(&1u32.to_le_bytes());
        bytes[24..32].copy_from_slice(&0x400000u64.to_le_bytes());
        bytes[32..40].copy_from_slice(&(ELF_HEADER_SIZE as u64).to_le_bytes());
        bytes[52..54].copy_from_slice(&(ELF_HEADER_SIZE as u16).to_le_bytes());
        bytes[54..56].copy_from_slice(&(PROGRAM_HEADER_SIZE as u16).to_le_bytes());
        bytes[56..58].copy_from_slice(&1u16.to_le_bytes());
        let ph = ELF_HEADER_SIZE;
        bytes[ph..ph + 4].copy_from_slice(&PT_LOAD.to_le_bytes());
        bytes[ph + 4..ph + 8].copy_from_slice(&flags.to_le_bytes());
        bytes[ph + 8..ph + 16].copy_from_slice(&0u64.to_le_bytes());
        bytes[ph + 16..ph + 24].copy_from_slice(&0x400000u64.to_le_bytes());
        bytes[ph + 32..ph + 40].copy_from_slice(&1u64.to_le_bytes());
        bytes[ph + 40..ph + 48].copy_from_slice(&1u64.to_le_bytes());
        bytes[ph + 48..ph + 56].copy_from_slice(&1u64.to_le_bytes());
        bytes
    }

    #[test]
    fn accepts_static_x86_64_executable() {
        let bytes = minimal_elf(PF_R | PF_X);
        let image = ElfImage::parse(&bytes).unwrap();
        assert_eq!(image.entry, 0x400000);
        assert_eq!(image.segments().len(), 1);
        assert_eq!(image.total_pages(), 1);
        assert!(image.segments()[0].is_executable());
    }

    #[test]
    fn rejects_truncated_and_bad_magic_images() {
        assert_eq!(ElfImage::parse(&[0u8; 12]), Err(ElfError::TruncatedHeader));
        let mut bytes = minimal_elf(PF_R | PF_X);
        bytes[0] = 0;
        assert_eq!(ElfImage::parse(&bytes), Err(ElfError::BadMagic));
    }

    #[test]
    fn rejects_writable_executable_segments() {
        let bytes = minimal_elf(PF_R | PF_W | PF_X);
        assert_eq!(ElfImage::parse(&bytes), Err(ElfError::WritableExecutableSegment));
    }

    #[test]
    fn rejects_entry_outside_executable_segment() {
        let bytes = minimal_elf(PF_R);
        assert_eq!(ElfImage::parse(&bytes), Err(ElfError::EntryNotExecutable));
    }

    #[test]
    fn rejects_segment_bytes_outside_file() {
        let mut bytes = minimal_elf(PF_R | PF_X);
        let ph = ELF_HEADER_SIZE;
        bytes[ph + 32..ph + 40].copy_from_slice(&0x1000u64.to_le_bytes());
        bytes[ph + 40..ph + 48].copy_from_slice(&0x1000u64.to_le_bytes());
        assert_eq!(ElfImage::parse(&bytes), Err(ElfError::SegmentOutOfBounds));
    }

    #[test]
    fn rejects_dynamic_images() {
        let mut bytes = minimal_elf(PF_R | PF_X);
        let ph = ELF_HEADER_SIZE;
        bytes[ph..ph + 4].copy_from_slice(&PT_INTERP.to_le_bytes());
        assert_eq!(ElfImage::parse(&bytes), Err(ElfError::UnsupportedProgramHeader));
    }
    #[test]
    fn rejects_program_header_table_outside_file() {
        let mut bytes = minimal_elf(PF_R | PF_X);
        bytes[32..40].copy_from_slice(&u64::MAX.to_le_bytes());
        assert_eq!(ElfImage::parse(&bytes), Err(ElfError::ProgramHeaderTableOutOfBounds));
    }

    #[test]
    fn rejects_empty_program_header_table() {
        let mut bytes = minimal_elf(PF_R | PF_X);
        bytes[56..58].copy_from_slice(&0u16.to_le_bytes());
        assert_eq!(ElfImage::parse(&bytes), Err(ElfError::EmptyProgramHeaderTable));
    }

    #[test]
    fn rejects_non_power_of_two_segment_alignment() {
        let mut bytes = minimal_elf(PF_R | PF_X);
        let ph = ELF_HEADER_SIZE;
        bytes[ph + 48..ph + 56].copy_from_slice(&3u64.to_le_bytes());
        assert_eq!(ElfImage::parse(&bytes), Err(ElfError::InvalidAlignment));
    }

    #[test]
    fn rejects_segment_virtual_address_overflow_or_kernel_range() {
        let mut bytes = minimal_elf(PF_R | PF_X);
        let ph = ELF_HEADER_SIZE;
        bytes[ph + 16..ph + 24].copy_from_slice(&0x7fff_ffff_ffffu64.to_le_bytes());
        bytes[ph + 40..ph + 48].copy_from_slice(&2u64.to_le_bytes());
        assert_eq!(ElfImage::parse(&bytes), Err(ElfError::SegmentOutOfBounds));
    }

    #[test]
    fn rejects_images_exceeding_page_budget() {
        let mut bytes = minimal_elf(PF_R | PF_X);
        let ph = ELF_HEADER_SIZE;
        bytes[ph + 40..ph + 48]
            .copy_from_slice(&((MAX_IMAGE_PAGES + 1) * PAGE_SIZE).to_le_bytes());
        assert_eq!(ElfImage::parse(&bytes), Err(ElfError::ImageTooLarge));
    }

    #[test]
    fn rejects_file_offset_addition_overflow() {
        let mut bytes = minimal_elf(PF_R | PF_X);
        let ph = ELF_HEADER_SIZE;
        bytes[ph + 8..ph + 16].copy_from_slice(&u64::MAX.to_le_bytes());
        bytes[ph + 32..ph + 40].copy_from_slice(&1u64.to_le_bytes());
        assert_eq!(ElfImage::parse(&bytes), Err(ElfError::SegmentOutOfBounds));
    }

    #[test]
    fn rejects_unsupported_elf_class_and_endian() {
        let mut bytes = minimal_elf(PF_R | PF_X);
        bytes[4] = 1;
        assert_eq!(ElfImage::parse(&bytes), Err(ElfError::UnsupportedClass));
        bytes[4] = 2;
        bytes[5] = 2;
        assert_eq!(ElfImage::parse(&bytes), Err(ElfError::UnsupportedEndian));
    }


    #[test] fn rejects_wrong_elf_type(){let mut b=minimal_elf(PF_R|PF_X);b[16..18].copy_from_slice(&3u16.to_le_bytes());assert_eq!(ElfImage::parse(&b),Err(ElfError::UnsupportedType));}

    #[test] fn rejects_wrong_machine(){let mut b=minimal_elf(PF_R|PF_X);b[18..20].copy_from_slice(&3u16.to_le_bytes());assert_eq!(ElfImage::parse(&b),Err(ElfError::UnsupportedMachine));}

    #[test] fn rejects_bad_ident_version(){let mut b=minimal_elf(PF_R|PF_X);b[6]=0;assert_eq!(ElfImage::parse(&b),Err(ElfError::UnsupportedVersion));}

    #[test] fn rejects_bad_header_version(){let mut b=minimal_elf(PF_R|PF_X);b[20..24].copy_from_slice(&2u32.to_le_bytes());assert_eq!(ElfImage::parse(&b),Err(ElfError::UnsupportedVersion));}

    #[test] fn rejects_wrong_header_size(){let mut b=minimal_elf(PF_R|PF_X);b[52..54].copy_from_slice(&63u16.to_le_bytes());assert_eq!(ElfImage::parse(&b),Err(ElfError::InvalidHeaderSize));}

    #[test] fn rejects_wrong_program_header_size(){let mut b=minimal_elf(PF_R|PF_X);b[54..56].copy_from_slice(&55u16.to_le_bytes());assert_eq!(ElfImage::parse(&b),Err(ElfError::InvalidProgramHeaderSize));}
}
