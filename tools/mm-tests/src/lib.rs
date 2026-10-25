//! Host-side test harness for pure memory-management algorithms.
//! This includes the exact virtual-range allocator source used by the kernel.

pub mod mm {
    pub mod frame {
        pub const FRAME_SIZE: u64 = 4096;
    }
}

#[path = "../../../kernel/src/mm/virtual_range.rs"]
pub mod virtual_range;
