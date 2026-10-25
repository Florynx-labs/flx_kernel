"""Regression tests for safety-critical physical frame allocator invariants.

These source-contract tests complement Rust unit tests: they guard the production
implementation against accidentally removing ownership, bounds, or accounting checks.
"""
from pathlib import Path
import re
import unittest


ROOT = Path(__file__).resolve().parents[2]
SOURCE = (ROOT / "kernel/src/mm/frame.rs").read_text(encoding="utf-8")


def function_body(source: str, signature: str) -> str:
    """Return a Rust function body using brace counting (handles nested blocks)."""
    start = source.find(signature)
    if start < 0:
        raise AssertionError(f"Rust function not found: {signature}")
    opening = source.find("{", start)
    if opening < 0:
        raise AssertionError(f"Opening brace not found: {signature}")
    depth = 0
    for index in range(opening, len(source)):
        if source[index] == "{":
            depth += 1
        elif source[index] == "}":
            depth -= 1
            if depth == 0:
                return source[opening + 1:index]
    raise AssertionError(f"Unclosed function body: {signature}")


class PhysicalFrameAllocatorContractTests(unittest.TestCase):
    def test_used_and_owned_bitmaps_are_separate(self):
        allocator = function_body(SOURCE, "pub struct BitmapFrameAllocator")
        self.assertIn("bitmap: &'static mut [u64]", allocator)
        self.assertIn("allocated: &'static mut [u64]", allocator)

    def test_allocation_marks_frame_used_and_owned(self):
        body = function_body(SOURCE, "fn allocate(&mut self)")
        self.assertRegex(body, r"self\.set_bit\(frame\)")
        self.assertRegex(body, r"self\.set_allocated\(frame, true\)")
        self.assertRegex(body, r"self\.free_frames -= 1")

    def test_free_rejects_misaligned_and_unmanaged_addresses_first(self):
        body = function_body(SOURCE, "fn deallocate(&mut self, addr: u64)")
        alignment_check = body.find("addr % FRAME_SIZE != 0")
        range_check = body.find("frame >= self.total_frames || addr < MIN_PHYS_ADDR")
        used_check = body.find("if !self.is_used(frame)")
        owned_check = body.find("if !self.is_allocated(frame)")
        self.assertGreaterEqual(alignment_check, 0)
        self.assertGreater(range_check, alignment_check)
        self.assertGreater(used_check, range_check)
        self.assertGreater(owned_check, used_check)

    def test_free_clears_ownership_and_used_state_and_updates_count(self):
        body = function_body(SOURCE, "fn deallocate(&mut self, addr: u64)")
        ownership_clear = body.find("self.set_allocated(frame, false)")
        used_clear = body.find("self.clear_bit(frame)")
        count_update = body.find("self.free_frames += 1")
        self.assertGreaterEqual(ownership_clear, 0)
        self.assertGreater(used_clear, ownership_clear)
        self.assertGreater(count_update, used_clear)

    def test_metadata_reserves_space_for_both_bitmaps(self):
        body = function_body(SOURCE, "unsafe fn new(regions: &[MemoryRegion], phys_offset: u64)")
        self.assertRegex(body, r"checked_mul\(16\)")
        self.assertIn("allocated_ptr", body)
        self.assertIn("metadata_frames", body)
        self.assertIn("alloc.set_bit(frame)", body)
        self.assertIn("alloc.free_frames -= 1", body)


if __name__ == "__main__":
    unittest.main(verbosity=2)
