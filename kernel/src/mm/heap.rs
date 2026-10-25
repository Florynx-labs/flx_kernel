//! Kernel heap with an early static arena and optional frame-backed growth.
//!
//! The static 1 MiB arena keeps allocations available during early boot. Once the physical
//! frame allocator is online, additional independent heap arenas are mapped into a reserved
//! kernel virtual window on demand. Arenas are page-backed and bounded so allocator failure
//! remains explicit instead of silently corrupting adjacent kernel memory.

use core::alloc::{GlobalAlloc, Layout};
use core::ptr;

use linked_list_allocator::Heap;

use crate::mm::{frame, paging::{ActivePageTable, PageFlags}};
use crate::sync::Spinlock;

/// Early heap capacity, available before physical memory management is initialized.
pub const HEAP_SIZE: usize = 1024 * 1024;
/// Default size of each additional heap arena.
pub const GROWTH_ARENA_MIN_SIZE: usize = 64 * 1024;
/// Largest single growth arena supported by this allocator.
pub const GROWTH_ARENA_MAX_SIZE: usize = 1024 * 1024;
/// Maximum number of separately mapped growth arenas.
pub const MAX_GROWTH_ARENAS: usize = 64;

const PAGE_SIZE: usize = frame::FRAME_SIZE as usize;
const MAX_ARENA_PAGES: usize = GROWTH_ARENA_MAX_SIZE / PAGE_SIZE;
const VIRTUAL_ARENA_START: u64 = 0x0000_4000_0000_0000;
const VIRTUAL_ARENA_STRIDE: u64 = GROWTH_ARENA_MAX_SIZE as u64;
const VIRTUAL_ARENA_WINDOW_SIZE: u64 = 256 * 1024 * 1024;
const VIRTUAL_CANDIDATE_LIMIT: usize = (VIRTUAL_ARENA_WINDOW_SIZE / VIRTUAL_ARENA_STRIDE) as usize;

#[repr(align(4096))]
struct HeapStorage([u8; HEAP_SIZE]);

static mut HEAP_STORAGE: HeapStorage = HeapStorage([0; HEAP_SIZE]);

struct HeapArena {
    heap: Heap,
    start: u64,
    size: usize,
    active: bool,
}

impl HeapArena {
    const fn empty() -> Self {
        Self { heap: Heap::empty(), start: 0, size: 0, active: false }
    }
}

struct HeapState {
    heap: Heap,
    initialized: bool,
    base_start: usize,
    base_size: usize,
    phys_offset: Option<u64>,
    next_virtual: u64,
    arena_count: usize,
    arenas: [HeapArena; MAX_GROWTH_ARENAS],
}

// Heap metadata stores pointers into exclusively owned arenas; Spinlock serializes access.
unsafe impl Send for HeapState {}

/// Thread-safe allocator usable as the kernel global allocator and in host tests.
pub struct KernelHeap {
    state: Spinlock<HeapState>,
}

impl KernelHeap {
    pub const fn new() -> Self {
        Self {
            state: Spinlock::new(HeapState {
                heap: Heap::empty(),
                initialized: false,
                base_start: 0,
                base_size: 0,
                phys_offset: None,
                next_virtual: VIRTUAL_ARENA_START,
                arena_count: 0,
                arenas: [const { HeapArena::empty() }; MAX_GROWTH_ARENAS],
            }),
        }
    }

    /// Initializes this allocator over a caller-owned arena.
    ///
    /// # Safety
    /// start..start + size must be valid, writable, exclusively reserved memory that
    /// remains mapped for the lifetime of this allocator and is not used for other purposes.
    pub unsafe fn init_region(&self, start: *mut u8, size: usize) -> Result<(), HeapInitError> {
        if start.is_null() || size == 0 {
            return Err(HeapInitError::InvalidRegion);
        }
        let mut state = self.state.lock();
        if state.initialized {
            return Err(HeapInitError::AlreadyInitialized);
        }
        // SAFETY: the caller guarantees the region's validity and exclusive ownership.
        unsafe { state.heap.init(start, size) };
        state.base_start = start as usize;
        state.base_size = size;
        state.initialized = true;
        Ok(())
    }

    /// Enables page-backed growth after the physical frame allocator is ready.
    ///
    /// The selected virtual window is probed against the active page tables before each arena
    /// is mapped; existing mappings are never overwritten.
    pub fn enable_growth(&self, phys_offset: u64) -> Result<(), HeapInitError> {
        let mut state = self.state.lock();
        if !state.initialized {
            return Err(HeapInitError::NotInitialized);
        }
        if state.phys_offset.is_some() {
            return Err(HeapInitError::GrowthAlreadyEnabled);
        }
        state.phys_offset = Some(phys_offset);
        Ok(())
    }

    /// Allocates a block. Returns null before initialization or when capacity is exhausted.
    pub fn allocate(&self, layout: Layout) -> *mut u8 {
        if layout.size() == 0 {
            return ptr::null_mut();
        }
        let mut state = self.state.lock();
        if !state.initialized {
            return ptr::null_mut();
        }

        if let Ok(allocation) = state.heap.allocate_first_fit(layout) {
            return allocation.as_ptr();
        }

        for arena in state.arenas.iter_mut().filter(|arena| arena.active) {
            if let Ok(allocation) = arena.heap.allocate_first_fit(layout) {
                return allocation.as_ptr();
            }
        }

        grow_and_allocate(&mut state, layout)
    }

    /// Releases a block previously returned by this allocator.
    ///
    /// # Safety
    /// ptr and layout must exactly describe a live allocation from this allocator.
    pub unsafe fn deallocate(&self, ptr: *mut u8, layout: Layout) {
        if ptr.is_null() || layout.size() == 0 {
            return;
        }
        let address = ptr as usize;
        let mut state = self.state.lock();
        if !state.initialized {
            return;
        }

        if address >= state.base_start && address < state.base_start.saturating_add(state.base_size) {
            // SAFETY: caller guarantees the allocation and layout are valid for this heap.
            unsafe {
                if let Some(non_null) = core::ptr::NonNull::new(ptr) {
                    state.heap.deallocate(non_null, layout);
                }
            }
            return;
        }

        for arena in state.arenas.iter_mut().filter(|arena| arena.active) {
            let start = arena.start;
            if (address as u64) >= start && (address as u64) < start.saturating_add(arena.size as u64) {
                // SAFETY: caller guarantees the allocation and layout are valid for this arena.
                unsafe {
                    if let Some(non_null) = core::ptr::NonNull::new(ptr) {
                        arena.heap.deallocate(non_null, layout);
                    }
                }
                return;
            }
        }
        // An invalid pointer is outside GlobalAlloc's safety contract; avoid corrupting a heap.
    }

    pub fn is_initialized(&self) -> bool {
        self.state.lock().initialized
    }

    pub fn growth_enabled(&self) -> bool {
        self.state.lock().phys_offset.is_some()
    }

    pub fn growth_arena_count(&self) -> usize {
        self.state.lock().arena_count
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeapInitError {
    InvalidRegion,
    AlreadyInitialized,
    NotInitialized,
    GrowthAlreadyEnabled,
}

/// Initializes the global allocator over its statically reserved 1 MiB early arena.
pub fn init(allocator: &KernelHeap) -> Result<(), HeapInitError> {
    // SAFETY: this static BSS region is owned exclusively by the heap for kernel lifetime.
    let start = unsafe { core::ptr::addr_of_mut!(HEAP_STORAGE.0).cast::<u8>() };
    // SAFETY: HEAP_STORAGE is static, writable, and aligned to the page size.
    unsafe { allocator.init_region(start, HEAP_SIZE) }
}

unsafe impl GlobalAlloc for KernelHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        self.allocate(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: GlobalAlloc callers must pass a pointer and layout returned by this allocator.
        unsafe { self.deallocate(ptr, layout) };
    }
}

fn growth_arena_size(layout: Layout) -> Option<usize> {
    if layout.align() > GROWTH_ARENA_MAX_SIZE {
        return None;
    }
    let required = layout.size().checked_add(layout.align())?.checked_add(64)?;
    let minimum = required.max(GROWTH_ARENA_MIN_SIZE);
    let rounded = minimum.checked_add(PAGE_SIZE - 1)? & !(PAGE_SIZE - 1);
    (rounded <= GROWTH_ARENA_MAX_SIZE).then_some(rounded)
}

fn grow_and_allocate(state: &mut HeapState, layout: Layout) -> *mut u8 {
    let Some(phys_offset) = state.phys_offset else {
        return ptr::null_mut();
    };
    if state.arena_count >= MAX_GROWTH_ARENAS {
        return ptr::null_mut();
    }
    let Some(arena_size) = growth_arena_size(layout) else {
        return ptr::null_mut();
    };
    let page_count = arena_size / PAGE_SIZE;

    // Open only the active page-table tree; this does not replace CR3.
    // SAFETY: phys_offset came from the bootloader's dynamic physical memory mapping.
    let mut page_table = unsafe { ActivePageTable::current(phys_offset) };

    // Find a vacant contiguous virtual range before taking physical frames.
    let mut candidate = state.next_virtual;
    let mut chosen = None;
    for _ in 0..VIRTUAL_CANDIDATE_LIMIT {
        let window_end = VIRTUAL_ARENA_START + VIRTUAL_ARENA_WINDOW_SIZE;
        let candidate_end = match candidate.checked_add(arena_size as u64) {
            Some(end) if end <= window_end => end,
            _ => break,
        };
        let mut vacant = true;
        for page in 0..page_count {
            let address = candidate + (page * PAGE_SIZE) as u64;
            match page_table.translate(address) {
                Ok(None) => {}
                _ => {
                    vacant = false;
                    break;
                }
            }
        }
        if vacant {
            // Keep the checked end calculation explicit for overflow/canonical-range safety.
            let _ = candidate_end;
            chosen = Some(candidate);
            break;
        }
        candidate = candidate.saturating_add(VIRTUAL_ARENA_STRIDE);
    }
    let Some(virtual_start) = chosen else {
        return ptr::null_mut();
    };

    let mut physical_frames = [0u64; MAX_ARENA_PAGES];
    let mut allocated = 0usize;
    while allocated < page_count {
        let Some(frame) = frame::alloc_frame() else {
            for physical in physical_frames[..allocated].iter().copied() {
                let _ = frame::free_frame(physical);
            }
            return ptr::null_mut();
        };
        physical_frames[allocated] = frame;
        // SAFETY: the bootloader's physical-memory mapping makes each allocated frame writable.
        unsafe {
            core::ptr::write_bytes((phys_offset + frame) as *mut u8, 0, PAGE_SIZE);
        }
        allocated += 1;
    }

    if page_table
        .map_4k_range(virtual_start, &physical_frames[..page_count], PageFlags::KERNEL_DATA)
        .is_err()
    {
        // map_4k_range rolls back any leaf mappings it created; frame ownership stays here.
        for physical in physical_frames[..page_count].iter().copied() {
            let _ = frame::free_frame(physical);
        }
        return ptr::null_mut();
    }

    let Some(slot) = state.arenas.iter_mut().find(|arena| !arena.active) else {
        // Defensive: the caller checked arena_count, but keep failure safe if state changes.
        for page in 0..page_count {
            let _ = page_table.unmap_4k(virtual_start + (page * PAGE_SIZE) as u64);
            let _ = frame::free_frame(physical_frames[page]);
        }
        return ptr::null_mut();
    };

    // SAFETY: the full virtual region is now mapped to exclusively owned, writable frames.
    unsafe { slot.heap.init(virtual_start as *mut u8, arena_size) };
    slot.start = virtual_start;
    slot.size = arena_size;
    slot.active = true;
    state.arena_count += 1;
    state.next_virtual = virtual_start.saturating_add(VIRTUAL_ARENA_STRIDE);

    slot.heap
        .allocate_first_fit(layout)
        .map_or(ptr::null_mut(), |allocation| allocation.as_ptr())
}

/// Forces at least one frame-backed arena to be created and verifies both allocations
/// remain independently freeable. Run only after growth has been enabled.
pub fn growth_self_test(allocator: &KernelHeap) -> Result<(), &'static str> {
    if !allocator.growth_enabled() {
        return Err("heap growth is not enabled");
    }
    let before = allocator.growth_arena_count();
    let layout = Layout::from_size_align(600 * 1024, 16).map_err(|_| "invalid growth test layout")?;
    let first = allocator.allocate(layout);
    if first.is_null() {
        return Err("first large heap allocation failed");
    }
    let second = allocator.allocate(layout);
    if second.is_null() {
        // SAFETY: first was returned by this allocator with this exact layout.
        unsafe { allocator.deallocate(first, layout) };
        return Err("second large heap allocation failed");
    }

    let grew = allocator.growth_arena_count() > before;
    // SAFETY: both pointers were returned by this allocator with this exact layout.
    unsafe {
        allocator.deallocate(first, layout);
        allocator.deallocate(second, layout);
    }
    if !grew {
        return Err("large allocations did not create a frame-backed arena");
    }
    Ok(())
}

/// Smoke test for the live heap, including alignment and reuse after free.
pub fn self_test(allocator: &KernelHeap) -> Result<(), &'static str> {
    let layout = Layout::from_size_align(96, 32).map_err(|_| "invalid heap test layout")?;
    let first = allocator.allocate(layout);
    if first.is_null() {
        return Err("heap allocation failed");
    }
    if (first as usize) % 32 != 0 {
        // SAFETY: first was just returned for this exact layout.
        unsafe { allocator.deallocate(first, layout) };
        return Err("heap returned misaligned pointer");
    }

    let second_layout = Layout::from_size_align(256, 16).map_err(|_| "invalid heap test layout")?;
    let second = allocator.allocate(second_layout);
    if second.is_null() || second == first {
        // SAFETY: each non-null pointer was returned by the allocator with the matching layout.
        unsafe {
            allocator.deallocate(first, layout);
            if !second.is_null() {
                allocator.deallocate(second, second_layout);
            }
        }
        return Err("heap returned null or overlapping allocations");
    }

    // SAFETY: both pointers are live allocations returned with these exact layouts.
    unsafe {
        allocator.deallocate(first, layout);
        allocator.deallocate(second, second_layout);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{growth_arena_size, HeapInitError, KernelHeap, GROWTH_ARENA_MAX_SIZE, GROWTH_ARENA_MIN_SIZE};
    use core::alloc::Layout;

    #[repr(align(4096))]
    struct TestArena([u8; 32 * 1024]);

    #[test]
    fn allocations_respect_alignment_and_can_be_reused() {
        let heap = KernelHeap::new();
        let mut arena = Box::new(TestArena([0; 32 * 1024]));
        // SAFETY: arena is exclusively owned for the test and outlives all allocations.
        unsafe { heap.init_region(arena.0.as_mut_ptr(), arena.0.len()).unwrap(); }

        let layout = Layout::from_size_align(127, 64).unwrap();
        let first = heap.allocate(layout);
        assert!(!first.is_null());
        assert_eq!((first as usize) % 64, 0);
        let second_layout = Layout::from_size_align(256, 16).unwrap();
        let second = heap.allocate(second_layout);
        assert!(!second.is_null());
        assert_ne!(first, second);

        // SAFETY: both pointers are live allocations returned with these exact layouts.
        unsafe {
            heap.deallocate(first, layout);
            heap.deallocate(second, second_layout);
        }

        let reused = heap.allocate(layout);
        assert!(!reused.is_null());
        assert_eq!((reused as usize) % 64, 0);
    }

    #[test]
    fn rejects_double_initialization_and_exhausts_cleanly() {
        let heap = KernelHeap::new();
        let mut arena = Box::new(TestArena([0; 32 * 1024]));
        // SAFETY: arena is exclusively owned for the test and outlives heap usage.
        unsafe { heap.init_region(arena.0.as_mut_ptr(), arena.0.len()).unwrap(); }
        // SAFETY: same region remains valid; duplicate initialization must not touch it.
        assert_eq!(
            unsafe { heap.init_region(arena.0.as_mut_ptr(), arena.0.len()) },
            Err(HeapInitError::AlreadyInitialized)
        );

        let huge = Layout::from_size_align(64 * 1024, 16).unwrap();
        assert!(heap.allocate(huge).is_null());
    }

    #[test]
    fn uninitialized_heap_fails_without_dereferencing_memory() {
        let heap = KernelHeap::new();
        assert!(heap.allocate(Layout::from_size_align(8, 8).unwrap()).is_null());
        assert!(!heap.is_initialized());
        assert!(!heap.growth_enabled());
    }

    #[test]
    fn growth_must_be_enabled_after_initialization_and_only_once() {
        let heap = KernelHeap::new();
        assert_eq!(heap.enable_growth(0), Err(HeapInitError::NotInitialized));

        let mut arena = Box::new(TestArena([0; 32 * 1024]));
        // SAFETY: arena is exclusively owned for this test.
        unsafe { heap.init_region(arena.0.as_mut_ptr(), arena.0.len()).unwrap(); }
        assert_eq!(heap.enable_growth(0), Ok(()));
        assert!(heap.growth_enabled());
        assert_eq!(heap.enable_growth(0), Err(HeapInitError::GrowthAlreadyEnabled));
    }

    #[test]
    fn growth_arena_sizes_are_page_aligned_and_bounded() {
        let small = growth_arena_size(Layout::from_size_align(128, 16).unwrap()).unwrap();
        assert_eq!(small, GROWTH_ARENA_MIN_SIZE);
        assert_eq!(small % 4096, 0);

        let large = growth_arena_size(Layout::from_size_align(128 * 1024, 64).unwrap()).unwrap();
        assert!(large >= 128 * 1024);
        assert_eq!(large % 4096, 0);

        assert_eq!(growth_arena_size(Layout::from_size_align(GROWTH_ARENA_MAX_SIZE, 4096).unwrap()), None);
        assert_eq!(growth_arena_size(Layout::from_size_align(8, 2 * GROWTH_ARENA_MAX_SIZE).unwrap()), None);
    }
}
