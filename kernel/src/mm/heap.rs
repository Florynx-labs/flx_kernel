//! Fixed-capacity kernel heap backed by a statically mapped arena.
//!
//! This is an early heap: it does not grow by mapping new physical frames yet. Its arena
//! lives in the kernel image's BSS and is therefore mapped with the kernel at boot. A later
//! VM milestone can replace this backing store with a virtually contiguous, frame-backed heap.

use core::alloc::{GlobalAlloc, Layout};
use core::ptr;

use linked_list_allocator::Heap;

use crate::sync::Spinlock;

/// Initial heap capacity (1 MiB). Keep page-aligned for future page-backed expansion.
pub const HEAP_SIZE: usize = 1024 * 1024;

#[repr(align(4096))]
struct HeapStorage([u8; HEAP_SIZE]);

static mut HEAP_STORAGE: HeapStorage = HeapStorage([0; HEAP_SIZE]);

struct HeapState {
    heap: Heap,
    initialized: bool,
}

// The heap contains pointers into its exclusively owned arena. All access is serialized by
// Spinlock; the arena is static and remains mapped for the kernel's lifetime.
unsafe impl Send for HeapState {}

/// Thread-safe wrapper usable both as the kernel's global allocator and in host tests.
pub struct KernelHeap {
    state: Spinlock<HeapState>,
}

impl KernelHeap {
    pub const fn new() -> Self {
        Self {
            state: Spinlock::new(HeapState {
                heap: Heap::empty(),
                initialized: false,
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
        state.initialized = true;
        Ok(())
    }

    /// Allocates a block. Returns null before initialization or on exhaustion.
    pub fn allocate(&self, layout: Layout) -> *mut u8 {
        if layout.size() == 0 {
            return ptr::null_mut();
        }
        let mut state = self.state.lock();
        if !state.initialized {
            return ptr::null_mut();
        }
        state.heap.allocate_first_fit(layout).map_or(ptr::null_mut(), |p| p.as_ptr())
    }

    /// Releases a block previously returned by this allocator.
    ///
    /// # Safety
    /// ptr and layout must exactly describe a live allocation from this allocator.
    pub unsafe fn deallocate(&self, ptr: *mut u8, layout: Layout) {
        if ptr.is_null() || layout.size() == 0 {
            return;
        }
        let mut state = self.state.lock();
        if !state.initialized {
            return;
        }
        // SAFETY: upheld by this method's caller contract.
        if let Some(non_null) = core::ptr::NonNull::new(ptr) {
            unsafe { state.heap.deallocate(non_null, layout) };
        }
    }

    pub fn is_initialized(&self) -> bool {
        self.state.lock().initialized
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeapInitError {
    InvalidRegion,
    AlreadyInitialized,
}

/// Initializes the global kernel heap over its statically reserved 1 MiB arena.
pub fn init() -> Result<(), HeapInitError> {
    // SAFETY: this static BSS region is owned exclusively by the heap for kernel lifetime.
    let start = core::ptr::addr_of_mut!(HEAP_STORAGE.0).cast::<u8>();
    // SAFETY: HEAP_STORAGE is static, writable, and aligned to the page size.
    unsafe { GLOBAL_HEAP.init_region(start, HEAP_SIZE) }
}

/// Global allocator instance registered by the kernel entry point.
pub static GLOBAL_HEAP: KernelHeap = KernelHeap::new();

unsafe impl GlobalAlloc for KernelHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        self.allocate(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: GlobalAlloc callers must pass a pointer and layout returned by this allocator.
        unsafe { self.deallocate(ptr, layout) };
    }
}

#[cfg(test)]
mod tests {
    use super::{HeapInitError, KernelHeap};
    use core::alloc::Layout;

    #[repr(align(4096))]
    struct TestArena([u8; 32 * 1024]);

    #[test]
    fn allocations_respect_alignment_and_can_be_reused() {
        let heap = KernelHeap::new();
        let mut arena = Box::new(TestArena([0; 32 * 1024]));
        // SAFETY: arena is exclusively owned for the test and outlives all allocations.
        unsafe {
            heap.init_region(arena.0.as_mut_ptr(), arena.0.len()).unwrap();
        }

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
        unsafe { heap.init_region(arena.0.as_mut_ptr(), arena.0.len()).unwrap() };
        // SAFETY: the same region is still valid; this call must fail before touching it.
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
    }
}
