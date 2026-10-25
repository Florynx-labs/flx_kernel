//! Physical frame allocator (bitmap based).
//!
//! One bit per 4 KiB frame (1 = used, 0 = free) covering physical addresses from 0 up to
//! the end of the highest usable region. The bitmap itself is carved out of the first
//! usable region large enough to hold it and accessed through the bootloader's
//! physical-memory mapping. Frames below 1 MiB are never handed out.

use bootloader_api::info::{MemoryRegion, MemoryRegionKind};

use crate::sync::Spinlock;

/// Size of a physical frame in bytes.
pub const FRAME_SIZE: u64 = 4096;
/// Lowest physical address eligible for allocation (legacy / firmware area is skipped).
const MIN_PHYS_ADDR: u64 = 0x10_0000;

/// Errors returned by frame allocator operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameError {
    /// Allocator was not initialized.
    Uninitialized,
    /// No usable region can hold the bitmap.
    NoSpaceForBitmap,
    /// Address is not frame-aligned or outside the managed range.
    InvalidAddress,
    /// The frame being freed is not currently allocated.
    DoubleFree,
    /// The frame is reserved memory and was never handed out by this allocator.
    NotAllocated,
}

/// Snapshot of allocator statistics.
#[derive(Debug, Clone, Copy)]
pub struct FrameStats {
    pub total_frames: usize,
    pub free_frames: usize,
}

pub struct BitmapFrameAllocator {
    bitmap: &'static mut [u64],
    /// Tracks ownership separately from the used bitmap so reserved frames cannot be freed.
    allocated: &'static mut [u64],
    total_frames: usize,
    free_frames: usize,
    next_hint: usize,
}

const fn align_up(value: u64, align: u64) -> u64 {
    (value + align - 1) & !(align - 1)
}

const fn align_down(value: u64, align: u64) -> u64 {
    value & !(align - 1)
}

impl BitmapFrameAllocator {
    /// Builds the allocator from the bootloader memory map.
    ///
    /// # Safety
    /// `phys_offset` must be the virtual offset at which *all* physical memory in `regions`
    /// is linearly mapped and writable, and the `Usable` regions must really be unused.
    unsafe fn new(regions: &[MemoryRegion], phys_offset: u64) -> Result<Self, FrameError> {
        let max_addr = regions
            .iter()
            .filter(|r| r.kind == MemoryRegionKind::Usable)
            .map(|r| r.end)
            .max()
            .ok_or(FrameError::NoSpaceForBitmap)?;

        let total_frames = (align_up(max_addr, FRAME_SIZE) / FRAME_SIZE) as usize;
        let words = total_frames.div_ceil(64);
        // Two bitmaps: one records used/free frames; the other records frames actually
        // allocated by this allocator. This prevents freeing firmware/reserved memory.
        let metadata_bytes = (words as u64)
            .checked_mul(16)
            .ok_or(FrameError::NoSpaceForBitmap)?;
        let metadata_frames = align_up(metadata_bytes, FRAME_SIZE) / FRAME_SIZE;

        // Find a usable region (above MIN_PHYS_ADDR) that can host both bitmaps.
        let bitmap_phys = regions
            .iter()
            .filter(|r| r.kind == MemoryRegionKind::Usable)
            .find_map(|r| {
                let start = align_up(r.start.max(MIN_PHYS_ADDR), FRAME_SIZE);
                let end = align_down(r.end, FRAME_SIZE);
                (end > start && end - start >= metadata_frames * FRAME_SIZE).then_some(start)
            })
            .ok_or(FrameError::NoSpaceForBitmap)?;

        let bitmap_ptr = (phys_offset + bitmap_phys) as *mut u64;
        let bitmap = core::slice::from_raw_parts_mut(bitmap_ptr, words);
        bitmap.fill(u64::MAX); // everything used until proven free

        // Place the ownership bitmap directly after the used/free bitmap.
        let allocated_ptr = (phys_offset + bitmap_phys + (words as u64) * 8) as *mut u64;
        let allocated = core::slice::from_raw_parts_mut(allocated_ptr, words);
        allocated.fill(0);

        let mut alloc = Self {
            bitmap,
            allocated,
            total_frames,
            free_frames: 0,
            next_hint: 0,
        };

        // Free every frame of every usable region (>= MIN_PHYS_ADDR).
        for region in regions.iter().filter(|r| r.kind == MemoryRegionKind::Usable) {
            let start = align_up(region.start.max(MIN_PHYS_ADDR), FRAME_SIZE);
            let end = align_down(region.end, FRAME_SIZE);
            let mut addr = start;
            while addr < end {
                alloc.clear_bit((addr / FRAME_SIZE) as usize);
                alloc.free_frames += 1;
                addr += FRAME_SIZE;
            }
        }

        // The bitmap's own frames are in use.
        let first = (bitmap_phys / FRAME_SIZE) as usize;
        for frame in first..first + metadata_frames as usize {
            alloc.set_bit(frame);
            alloc.free_frames -= 1;
        }

        Ok(alloc)
    }

    #[inline]
    fn is_used(&self, frame: usize) -> bool {
        self.bitmap[frame / 64] & (1 << (frame % 64)) != 0
    }

    #[inline]
    fn is_allocated(&self, frame: usize) -> bool {
        self.allocated[frame / 64] & (1 << (frame % 64)) != 0
    }

    #[inline]
    fn set_allocated(&mut self, frame: usize, allocated: bool) {
        let mask = 1u64 << (frame % 64);
        if allocated {
            self.allocated[frame / 64] |= mask;
        } else {
            self.allocated[frame / 64] &= !mask;
        }
    }

    #[inline]
    fn set_bit(&mut self, frame: usize) {
        self.bitmap[frame / 64] |= 1 << (frame % 64);
    }

    #[inline]
    fn clear_bit(&mut self, frame: usize) {
        self.bitmap[frame / 64] &= !(1 << (frame % 64));
    }

    /// Allocates one frame; returns its physical address.
    fn allocate(&mut self) -> Option<u64> {
        let words = self.bitmap.len();
        let start_word = self.next_hint / 64;
        for i in 0..words {
            let w = (start_word + i) % words;
            let value = self.bitmap[w];
            if value != u64::MAX {
                let bit = (!value).trailing_zeros() as usize;
                let frame = w * 64 + bit;
                if frame >= self.total_frames {
                    continue; // padding bits past the end are always set, but stay defensive
                }
                self.set_bit(frame);
                self.set_allocated(frame, true);
                self.free_frames -= 1;
                self.next_hint = frame + 1;
                return Some(frame as u64 * FRAME_SIZE);
            }
        }
        None
    }

    /// Frees the frame at physical address `addr`.
    fn deallocate(&mut self, addr: u64) -> Result<(), FrameError> {
        if addr % FRAME_SIZE != 0 {
            return Err(FrameError::InvalidAddress);
        }
        let frame = (addr / FRAME_SIZE) as usize;
        if frame >= self.total_frames || addr < MIN_PHYS_ADDR {
            return Err(FrameError::InvalidAddress);
        }
        if !self.is_used(frame) {
            return Err(FrameError::DoubleFree);
        }
        if !self.is_allocated(frame) {
            return Err(FrameError::NotAllocated);
        }
        self.set_allocated(frame, false);
        self.clear_bit(frame);
        self.free_frames += 1;
        if frame < self.next_hint {
            self.next_hint = frame;
        }
        Ok(())
    }
}

static ALLOCATOR: Spinlock<Option<BitmapFrameAllocator>> = Spinlock::new(None);

/// Initializes the global frame allocator.
///
/// # Safety
/// See [`BitmapFrameAllocator::new`]: `phys_offset` must map all physical memory and the
/// `Usable` regions in `regions` must not be in use by anything else.
pub unsafe fn init(regions: &[MemoryRegion], phys_offset: u64) -> Result<FrameStats, FrameError> {
    let allocator = BitmapFrameAllocator::new(regions, phys_offset)?;
    let stats = FrameStats {
        total_frames: allocator.total_frames,
        free_frames: allocator.free_frames,
    };
    *ALLOCATOR.lock() = Some(allocator);
    Ok(stats)
}

/// Allocates one 4 KiB physical frame and returns its physical address.
pub fn alloc_frame() -> Option<u64> {
    ALLOCATOR.lock().as_mut()?.allocate()
}

/// Returns a previously allocated frame to the allocator.
pub fn free_frame(addr: u64) -> Result<(), FrameError> {
    ALLOCATOR
        .lock()
        .as_mut()
        .ok_or(FrameError::Uninitialized)?
        .deallocate(addr)
}

/// Current allocator statistics.
pub fn stats() -> Option<FrameStats> {
    ALLOCATOR.lock().as_ref().map(|a| FrameStats {
        total_frames: a.total_frames,
        free_frames: a.free_frames,
    })
}

/// Boot-time self-test: allocate, verify uniqueness/alignment, free, detect double free.
pub fn self_test() -> Result<(), &'static str> {
    let before = stats().ok_or("allocator not initialized")?.free_frames;

    let a = alloc_frame().ok_or("alloc a failed")?;
    let b = alloc_frame().ok_or("alloc b failed")?;
    let c = alloc_frame().ok_or("alloc c failed")?;

    if a == b || b == c || a == c {
        return Err("allocator returned duplicate frames");
    }
    if [a, b, c].iter().any(|f| f % FRAME_SIZE != 0 || *f < MIN_PHYS_ADDR) {
        return Err("allocator returned unaligned or low frame");
    }
    if stats().ok_or("stats")?.free_frames != before - 3 {
        return Err("free count wrong after alloc");
    }

    free_frame(b).map_err(|_| "free b failed")?;
    if free_frame(b) != Err(FrameError::DoubleFree) {
        return Err("double free not detected");
    }
    free_frame(a).map_err(|_| "free a failed")?;
    free_frame(c).map_err(|_| "free c failed")?;

    if stats().ok_or("stats")?.free_frames != before {
        return Err("free count wrong after free");
    }
    Ok(())
}


#[cfg(test)]
mod tests {
    use super::{BitmapFrameAllocator, FrameError, FRAME_SIZE, MIN_PHYS_ADDR};

    fn allocator_with_free_frames(frames: &[usize]) -> BitmapFrameAllocator {
        let total_frames = 512;
        let words = total_frames / 64;
        let mut used = vec![u64::MAX; words];
        let owned = vec![0u64; words];

        for &frame in frames {
            used[frame / 64] &= !(1u64 << (frame % 64));
        }

        BitmapFrameAllocator {
            bitmap: Box::leak(used.into_boxed_slice()),
            allocated: Box::leak(owned.into_boxed_slice()),
            total_frames,
            free_frames: frames.len(),
            next_hint: 0,
        }
    }

    #[test]
    fn reserved_used_frame_cannot_be_freed() {
        let mut allocator = allocator_with_free_frames(&[]);
        let reserved = 300 * FRAME_SIZE;

        assert!(reserved >= MIN_PHYS_ADDR);
        assert!(allocator.is_used(300));
        assert!(!allocator.is_allocated(300));
        assert_eq!(allocator.deallocate(reserved), Err(FrameError::NotAllocated));
        assert!(allocator.is_used(300));
        assert_eq!(allocator.free_frames, 0);
    }

    #[test]
    fn allocator_owned_frame_can_be_freed_once() {
        let mut allocator = allocator_with_free_frames(&[300]);
        let address = allocator.allocate().expect("one free frame should exist");

        assert_eq!(address, 300 * FRAME_SIZE);
        assert!(allocator.is_used(300));
        assert!(allocator.is_allocated(300));
        assert_eq!(allocator.free_frames, 0);

        allocator.deallocate(address).expect("owned frame should be freed");
        assert!(!allocator.is_used(300));
        assert!(!allocator.is_allocated(300));
        assert_eq!(allocator.free_frames, 1);
        assert_eq!(allocator.deallocate(address), Err(FrameError::DoubleFree));
    }

    #[test]
    fn invalid_free_does_not_change_allocator_state() {
        let mut allocator = allocator_with_free_frames(&[300]);
        let before = allocator.free_frames;

        assert_eq!(allocator.deallocate(300 * FRAME_SIZE + 1), Err(FrameError::InvalidAddress));
        assert_eq!(allocator.deallocate(100 * FRAME_SIZE), Err(FrameError::InvalidAddress));
        assert_eq!(allocator.free_frames, before);
        assert!(!allocator.is_used(300));
    }
}
