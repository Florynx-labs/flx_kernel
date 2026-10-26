//! Fixed-capacity virtual address range allocator.
//!
//! This allocator reserves virtual page ranges only; it does not create page-table
//! mappings. It is intended to manage explicitly selected, already-reserved virtual
//! windows (for example a future kernel heap arena) without requiring a heap itself.

use crate::mm::frame::FRAME_SIZE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VirtualRangeError {
    UnalignedRange,
    InvalidRange,
    BitmapTooSmall,
    ZeroPages,
    OutOfVirtualSpace,
    OutOfRange,
    AlreadyFree,
}

/// First-fit allocator over a caller-supplied virtual address interval.
/// A set bitmap bit means that virtual page is reserved.
pub struct VirtualRangeAllocator<'a> {
    start: u64,
    page_count: usize,
    bitmap: &'a mut [u64],
}

impl<'a> VirtualRangeAllocator<'a> {
    /// Creates an allocator for the half-open range [start, end).
    /// The bitmap is caller-owned so this works before the kernel heap exists.
    pub fn new(start: u64, end: u64, bitmap: &'a mut [u64]) -> Result<Self, VirtualRangeError> {
        if start % FRAME_SIZE != 0 || end % FRAME_SIZE != 0 {
            return Err(VirtualRangeError::UnalignedRange);
        }
        if end <= start {
            return Err(VirtualRangeError::InvalidRange);
        }
        let last = end - 1;
        if !is_canonical(start)
            || !is_canonical(last)
            || ((start >> 47) & 1) != ((last >> 47) & 1)
        {
            return Err(VirtualRangeError::InvalidRange);
        }
        let page_count_u64 = (end - start) / FRAME_SIZE;
        let page_count = usize::try_from(page_count_u64).map_err(|_| VirtualRangeError::InvalidRange)?;
        let required_words = page_count.div_ceil(64);
        if bitmap.len() < required_words {
            return Err(VirtualRangeError::BitmapTooSmall);
        }
        bitmap.fill(0);
        Ok(Self { start, page_count, bitmap })
    }

    /// Reserves pages contiguous virtual pages and returns their starting address.
    pub fn allocate(&mut self, pages: usize) -> Result<u64, VirtualRangeError> {
        if pages == 0 {
            return Err(VirtualRangeError::ZeroPages);
        }
        if pages > self.page_count {
            return Err(VirtualRangeError::OutOfVirtualSpace);
        }

        let mut run_start = 0usize;
        let mut run_len = 0usize;
        for page in 0..self.page_count {
            if self.is_used(page) {
                run_start = page + 1;
                run_len = 0;
            } else {
                run_len += 1;
                if run_len == pages {
                    for allocated in run_start..run_start + pages {
                        self.set_used(allocated, true);
                    }
                    return self.address_of(run_start);
                }
            }
        }
        Err(VirtualRangeError::OutOfVirtualSpace)
    }

    /// Releases a reserved range. It does not unmap pages or free physical frames.
    pub fn free(&mut self, address: u64, pages: usize) -> Result<(), VirtualRangeError> {
        if pages == 0 {
            return Err(VirtualRangeError::ZeroPages);
        }
        if address < self.start || address % FRAME_SIZE != 0 {
            return Err(VirtualRangeError::OutOfRange);
        }
        let offset = address.checked_sub(self.start).ok_or(VirtualRangeError::OutOfRange)?;
        let first = usize::try_from(offset / FRAME_SIZE).map_err(|_| VirtualRangeError::OutOfRange)?;
        let end = first.checked_add(pages).ok_or(VirtualRangeError::OutOfRange)?;
        if end > self.page_count {
            return Err(VirtualRangeError::OutOfRange);
        }
        if (first..end).any(|page| !self.is_used(page)) {
            return Err(VirtualRangeError::AlreadyFree);
        }
        for page in first..end {
            self.set_used(page, false);
        }
        Ok(())
    }

    pub fn capacity_pages(&self) -> usize {
        self.page_count
    }

    pub fn used_pages(&self) -> usize {
        (0..self.page_count).filter(|page| self.is_used(*page)).count()
    }

    #[inline]
    fn address_of(&self, page: usize) -> Result<u64, VirtualRangeError> {
        let offset = u64::try_from(page).ok()
            .and_then(|p| p.checked_mul(FRAME_SIZE))
            .ok_or(VirtualRangeError::InvalidRange)?;
        self.start.checked_add(offset).ok_or(VirtualRangeError::InvalidRange)
    }

    #[inline]
    fn is_used(&self, page: usize) -> bool {
        self.bitmap[page / 64] & (1u64 << (page % 64)) != 0
    }

    #[inline]
    fn set_used(&mut self, page: usize, used: bool) {
        let bit = 1u64 << (page % 64);
        if used {
            self.bitmap[page / 64] |= bit;
        } else {
            self.bitmap[page / 64] &= !bit;
        }
    }
}

#[inline]
fn is_canonical(address: u64) -> bool {
    let upper = address >> 48;
    let sign = (address >> 47) & 1;
    if sign == 0 { upper == 0 } else { upper == 0xffff }
}

#[cfg(test)]
mod tests {
    use super::{VirtualRangeAllocator, VirtualRangeError};
    use crate::mm::frame::FRAME_SIZE;

    #[test]
    fn allocates_contiguous_ranges_and_reuses_freed_space() {
        let mut bitmap = [0u64; 1];
        let mut range = VirtualRangeAllocator::new(0x1000_0000, 0x1000_0000 + 16 * FRAME_SIZE, &mut bitmap).unwrap();
        let a = range.allocate(3).unwrap();
        let b = range.allocate(2).unwrap();
        assert_eq!(a, 0x1000_0000);
        assert_eq!(b, a + 3 * FRAME_SIZE);
        assert_eq!(range.used_pages(), 5);
        range.free(a, 3).unwrap();
        assert_eq!(range.allocate(2).unwrap(), a);
        assert_eq!(range.used_pages(), 4);
    }

    #[test]
    fn rejects_double_free_and_out_of_range() {
        let mut bitmap = [0u64; 1];
        let mut range = VirtualRangeAllocator::new(0x2000_0000, 0x2000_0000 + 8 * FRAME_SIZE, &mut bitmap).unwrap();
        let page = range.allocate(1).unwrap();
        range.free(page, 1).unwrap();
        assert_eq!(range.free(page, 1), Err(VirtualRangeError::AlreadyFree));
        assert_eq!(range.free(0x3000_0000, 1), Err(VirtualRangeError::OutOfRange));
    }

    #[test]
    fn handles_fragmentation_and_exhaustion() {
        let mut bitmap = [0u64; 1];
        let mut range = VirtualRangeAllocator::new(
            0x4000_0000,
            0x4000_0000 + 8 * FRAME_SIZE,
            &mut bitmap,
        )
        .unwrap();

        let a = range.allocate(2).unwrap();
        let b = range.allocate(2).unwrap();
        let c = range.allocate(2).unwrap();
        let _d = range.allocate(2).unwrap();
        assert_eq!(range.allocate(1), Err(VirtualRangeError::OutOfVirtualSpace));

        range.free(a, 2).unwrap();
        range.free(c, 2).unwrap();
        // There are four free pages in total, but no contiguous run of three.
        assert_eq!(range.allocate(3), Err(VirtualRangeError::OutOfVirtualSpace));

        range.free(b, 2).unwrap();
        assert_eq!(range.allocate(5).unwrap(), 0x4000_0000);
    }

    #[test]
    fn rejects_zero_page_requests() {
        let mut bitmap = [0u64; 1];
        let mut range = VirtualRangeAllocator::new(
            0x5000_0000,
            0x5000_0000 + 4 * FRAME_SIZE,
            &mut bitmap,
        )
        .unwrap();
        assert_eq!(range.allocate(0), Err(VirtualRangeError::ZeroPages));
        assert_eq!(range.free(0x5000_0000, 0), Err(VirtualRangeError::ZeroPages));
    }

    #[test]
    fn validates_range_and_bitmap() {
        let mut small = [0u64; 1];
        assert_eq!(VirtualRangeAllocator::new(0x1001, 0x3000, &mut small).err(), Some(VirtualRangeError::UnalignedRange));
        assert_eq!(VirtualRangeAllocator::new(0x1000, 0x1000, &mut small).err(), Some(VirtualRangeError::InvalidRange));
        assert_eq!(VirtualRangeAllocator::new(0x1000, 0x1000 + 65 * FRAME_SIZE, &mut small).err(), Some(VirtualRangeError::BitmapTooSmall));
        assert_eq!(VirtualRangeAllocator::new(0x0000_7fff_ffff_f000, 0xffff_8000_0000_0000, &mut small).err(), Some(VirtualRangeError::InvalidRange));
    }

    #[test] fn rejects_unaligned_range_end(){let mut b=[0u64;1];assert_eq!(VirtualRangeAllocator::new(0x1000,0x2001,&mut b).err(),Some(VirtualRangeError::UnalignedRange));}

    #[test] fn rejects_reversed_range(){let mut b=[0u64;1];assert_eq!(VirtualRangeAllocator::new(0x3000,0x2000,&mut b).err(),Some(VirtualRangeError::InvalidRange));}
}
