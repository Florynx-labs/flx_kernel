//! Host-side test harness for pure memory-management algorithms.
//! This includes the exact virtual-range allocator source used by the kernel.

pub mod mm {
    pub mod frame {
        pub const FRAME_SIZE: u64 = 4096;

        // Host tests exercise pure address logic only; privileged paging paths are never called.
        pub fn alloc_frame() -> Option<u64> {
            None
        }
    }
}

#[path = "../../../kernel/src/mm/virtual_range.rs"]
pub mod virtual_range;

#[path = "../../../kernel/src/mm/paging.rs"]
pub mod paging;
