//! Reproducible host-side microbenchmarks for production memory-management code.
//!
//! These measurements exercise the production allocator algorithms through the host
//! test harness. The harness uses std::Mutex in place of the kernel IRQ-safe spinlock,
//! so results are NOT kernel/QEMU timings and must never be reported as such.

use flx_mm_tests::heap::KernelHeap;
use flx_mm_tests::mm::frame::{self, FrameMemoryRegion};
use flx_mm_tests::mm::mapper::PhysicalMemoryMapper;
use std::alloc::Layout as StdLayout;
use std::env;
use std::ptr::NonNull;
use std::time::Instant;

const WARMUP_BATCHES: usize = 5;
const SAMPLES: usize = 30;
const OPS_PER_BATCH: usize = 512;
const PHYSICAL_START: u64 = 0x10_0000;
const BACKING_BYTES: usize = 32 * 1024 * 1024;

#[repr(align(4096))]
struct AlignedBacking([u8; BACKING_BYTES]);

struct Region {
    start: u64,
    end: u64,
}

impl FrameMemoryRegion for Region {
    fn start(&self) -> u64 { self.start }
    fn end(&self) -> u64 { self.end }
    fn is_usable(&self) -> bool { true }
}

struct HostMapper {
    base: u64,
    length: usize,
    // Leaked for the benchmark process: the allocator stores mapped bitmap slices as 'static.
    backing: NonNull<u8>,
}

// SAFETY: this mapper owns a stable, writable host allocation for the full benchmark run.
// It is a host-only test mapping and is never linked into the kernel image.
unsafe impl PhysicalMemoryMapper for HostMapper {
    unsafe fn map_physical(&self, address: u64, size: usize) -> Option<NonNull<u8>> {
        let offset = usize::try_from(address.checked_sub(self.base)?).ok()?;
        let end = offset.checked_add(size)?;
        if end > self.length { return None; }

        // SAFETY: the bounds check proves the requested range is inside the owned backing.
        NonNull::new(unsafe { self.backing.as_ptr().add(offset) })
    }
}

fn run_samples<F>(name: &str, units_per_batch: usize, mut batch: F)
where
    F: FnMut() -> u128,
{
    let mut samples = Vec::with_capacity(SAMPLES);
    for index in 0..(WARMUP_BATCHES + SAMPLES) {
        let elapsed_ns = batch();
        if index >= WARMUP_BATCHES {
            let sample_index = index - WARMUP_BATCHES;
            println!(
                "{name},host,{},{}-{},{},{elapsed_ns},ns,ops_per_sample={units_per_batch}",
                env::var("FLX_BENCH_COMMIT").unwrap_or_else(|_| "unknown".into()),
                env::consts::OS,
                env::consts::ARCH,
                sample_index
            );
            samples.push(elapsed_ns);
        }
    }

    samples.sort_unstable();
    let median = percentile(&samples, 50);
    let p95 = percentile(&samples, 95);
    let p99 = percentile(&samples, 99);
    eprintln!(
        "# summary benchmark={name} samples={} median_ns={median} p95_ns={p95} p99_ns={p99} min_ns={} max_ns={}",
        samples.len(),
        samples[0],
        samples[samples.len() - 1]
    );
}

fn percentile(sorted: &[u128], percentile: usize) -> u128 {
    // Nearest-rank percentile: rank = ceil(p * N / 100), converted to zero-based index.
    let rank = (percentile * sorted.len() + 99) / 100;
    sorted[rank.saturating_sub(1).min(sorted.len() - 1)]
}

/// Allocate zeroed storage for byte-only benchmark backing structs.
///
/// # Safety
/// `T` must permit an all-zero byte representation (true for the two byte-array wrappers below).
unsafe fn boxed_zeroed<T>() -> Box<T> {
    let layout = StdLayout::new::<T>();
    // SAFETY: alloc_zeroed returns a suitably aligned allocation for this exact layout.
    let pointer = unsafe { std::alloc::alloc_zeroed(layout).cast::<T>() };
    if pointer.is_null() {
        std::alloc::handle_alloc_error(layout);
    }
    // SAFETY: pointer was allocated with layout and is initialized to all-zero bytes.
    unsafe { Box::from_raw(pointer) }
}

fn benchmark_frame_alloc_free() {
    // The frame allocator intentionally keeps its metadata mapped for kernel lifetime.
    // Leak this host buffer and retain only its raw pointer to avoid shared-reference aliasing.
    // SAFETY: AlignedBacking contains only u8 bytes, so all-zero is a valid value.
    let backing = Box::leak(unsafe { boxed_zeroed::<AlignedBacking>() });
    let backing_pointer = NonNull::new(backing.0.as_mut_ptr()).expect("non-empty backing");
    let mapper = HostMapper {
        base: PHYSICAL_START,
        length: BACKING_BYTES,
        backing: backing_pointer,
    };
    let region = Region {
        start: PHYSICAL_START,
        end: PHYSICAL_START + BACKING_BYTES as u64,
    };

    // SAFETY: the region is a dedicated host buffer and the mapper bounds-checks every range.
    unsafe { frame::init(&[region], &mapper) }.expect("initialize host frame allocator");
    let mut allocated = Vec::with_capacity(OPS_PER_BATCH);

    run_samples("frame_alloc_free", OPS_PER_BATCH, || {
        allocated.clear();
        let start = Instant::now();
        for _ in 0..OPS_PER_BATCH {
            allocated.push(frame::alloc_frame().expect("frame allocator exhausted"));
        }
        for address in allocated.drain(..) {
            frame::free_frame(address).expect("free frame returned by this sample");
        }
        start.elapsed().as_nanos()
    });
}

#[repr(align(4096))]
struct HeapBacking([u8; 2 * 1024 * 1024]);

fn benchmark_heap_alloc_free(size: usize, alignment: usize, name: &'static str) {
    // SAFETY: HeapBacking contains only u8 bytes, so all-zero is a valid value.
    let mut backing = unsafe { boxed_zeroed::<HeapBacking>() };
    let heap = KernelHeap::new();

    // SAFETY: the aligned backing is exclusively owned by this heap and outlives it.
    unsafe { heap.init_region(backing.0.as_mut_ptr(), backing.0.len()) }
        .expect("initialize host heap");

    let layout = StdLayout::from_size_align(size, alignment).expect("valid benchmark layout");
    let mut allocations = Vec::with_capacity(OPS_PER_BATCH);

    run_samples(name, OPS_PER_BATCH, || {
        allocations.clear();
        let start = Instant::now();
        for _ in 0..OPS_PER_BATCH {
            let pointer = heap.allocate(layout);
            assert!(!pointer.is_null(), "host heap exhausted during benchmark");
            allocations.push(pointer);
        }
        for pointer in allocations.drain(..) {
            // SAFETY: each pointer was allocated by this heap with this exact layout.
            unsafe { heap.deallocate(pointer, layout) };
        }
        start.elapsed().as_nanos()
    });
}

fn main() {
    println!("benchmark,target,commit,environment,iteration,elapsed_ns,unit,notes");
    eprintln!("# host-only benchmark; lock backend is std::Mutex, not the kernel spinlock");
    eprintln!("# rustc={}", env::var("FLX_BENCH_RUSTC").unwrap_or_else(|_| "unknown".into()));
    eprintln!("# rustc_host={}", env::var("FLX_BENCH_RUSTC_HOST").unwrap_or_else(|_| "unknown".into()));
    eprintln!("# commit={}", env::var("FLX_BENCH_COMMIT").unwrap_or_else(|_| "unknown".into()));
    eprintln!("# worktree_clean={}", env::var("FLX_BENCH_CLEAN").unwrap_or_else(|_| "unknown".into()));
    eprintln!("# worktree_status={}", env::var("FLX_BENCH_WORKTREE_STATUS").unwrap_or_else(|_| "clean".into()));
    eprintln!("# host_kernel={}", env::var("FLX_BENCH_HOST_KERNEL").unwrap_or_else(|_| "unknown".into()));
    eprintln!("# host_cpu={}", env::var("FLX_BENCH_HOST_CPU").unwrap_or_else(|_| "unknown".into()));
    eprintln!("# warmup_batches={WARMUP_BATCHES} measured_batches={SAMPLES} operations_per_batch={OPS_PER_BATCH}");

    benchmark_frame_alloc_free();
    benchmark_heap_alloc_free(32, 8, "heap_alloc_free_32b");
    benchmark_heap_alloc_free(128, 16, "heap_alloc_free_128b");
    benchmark_heap_alloc_free(512, 64, "heap_alloc_free_512b");
}
