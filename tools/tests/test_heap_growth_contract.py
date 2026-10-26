"""Source-contract tests for frame-backed kernel heap growth."""
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
HEAP = (ROOT / "kernel/src/mm/heap.rs").read_text(encoding="utf-8")
MAIN = (ROOT / "kernel/src/main.rs").read_text(encoding="utf-8")


def function_body(source: str, signature: str) -> str:
    start = source.find(signature)
    if start < 0:
        raise AssertionError(f"Rust function not found: {signature}")
    opening = source.find("{", start)
    depth = 0
    for index in range(opening, len(source)):
        if source[index] == "{":
            depth += 1
        elif source[index] == "}":
            depth -= 1
            if depth == 0:
                return source[opening + 1:index]
    raise AssertionError(f"Unclosed Rust function: {signature}")


class HeapGrowthContractTests(unittest.TestCase):
    def test_growth_is_bounded_and_keeps_early_heap(self):
        self.assertIn("pub const HEAP_SIZE: usize = 1024 * 1024;", HEAP)
        self.assertIn("pub const MAX_GROWTH_ARENAS: usize = 64;", HEAP)
        self.assertIn("pub const GROWTH_ARENA_MAX_SIZE: usize = 1024 * 1024;", HEAP)
        self.assertIn("static mut HEAP_STORAGE: HeapStorage", HEAP)

    def test_growth_requires_initialized_heap_and_is_enabled_once(self):
        body = function_body(HEAP, "pub fn enable_growth(&self, phys_offset: u64)")
        self.assertIn("HeapInitError::NotInitialized", body)
        self.assertIn("HeapInitError::GrowthAlreadyEnabled", body)
        self.assertIn("state.phys_offset = Some(phys_offset)", body)

    def test_growth_maps_frames_and_releases_them_on_failure(self):
        body = function_body(HEAP, "fn grow_and_allocate(state: &mut HeapState, layout: Layout)")
        self.assertIn("frame::alloc_frame()", body)
        self.assertIn(".map_4k_range(virtual_start", body)
        self.assertIn("PageFlags::KERNEL_DATA", body)
        self.assertIn("frame::free_frame(physical)", body)
        self.assertIn("write_bytes", body)

    def test_deallocation_routes_to_the_owning_arena(self):
        body = function_body(HEAP, "pub unsafe fn deallocate(&self, ptr: *mut u8, layout: Layout)")
        self.assertIn("state.base_start", body)
        self.assertIn("arena.start", body)
        self.assertIn("arena.heap.deallocate", body)

    def test_growth_is_enabled_only_after_frame_allocator_initializes(self):
        frame_success = MAIN.find("Ok(stats) =>")
        growth_enable = MAIN.find("GLOBAL_ALLOCATOR.enable_growth(offset)")
        self.assertGreaterEqual(frame_success, 0)
        self.assertGreater(growth_enable, frame_success)


if __name__ == "__main__":
    unittest.main(verbosity=2)
