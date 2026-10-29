"""Source-contract tests for isolated user address spaces."""
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
PAGING = (ROOT / "kernel/src/mm/paging.rs").read_text(encoding="utf-8")


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


class AddressSpaceContractTests(unittest.TestCase):
    def test_address_space_has_bounded_user_mapping_tracking(self):
        self.assertIn("pub const MAX_USER_PAGES: usize = 256;", PAGING)
        self.assertIn("mapped_pages: [u64; MAX_USER_PAGES]", PAGING)
        self.assertIn("mapped_count: usize", PAGING)

    def test_new_root_preserves_kernel_slots_but_keeps_user_slots_private(self):
        body = function_body(PAGING, "pub unsafe fn new(phys_offset: u64)")
        self.assertIn("for index in 0..ENTRY_COUNT", body)
        self.assertIn("let phys_map_index = indices(phys_offset)[0];", body)
        self.assertIn("index < ENTRY_COUNT / 2 && index != 2 && index != phys_map_index", body)
        self.assertIn("entry & !USER", body)
        self.assertIn("frame::alloc_frame()", body)
        # The kernel image lives in lower-half PML4 slot 2, but user code (0)
        # and user stack (255) must not inherit bootloader page-table trees.
        self.assertIn("which is PML4 index 2", body)
        self.assertIn("user code (index 0) and", body)
        self.assertIn("user stack (index 255)", body)

    def test_user_pages_are_lower_half_and_nonzero(self):
        body = function_body(PAGING, "fn validate_user_page_address(address: u64)")
        self.assertIn("USER_MIN_ADDRESS", body)
        self.assertIn("USER_ADDRESS_LIMIT", body)
        self.assertIn("PagingError::UnalignedAddress", body)

    def test_user_data_and_code_have_distinct_wx_permissions(self):
        self.assertIn("pub const USER_DATA: Self = Self { writable: true, user: true, no_execute: true };", PAGING)
        self.assertIn("pub const USER_CODE: Self = Self { writable: false, user: true, no_execute: false };", PAGING)
        data = function_body(PAGING, "pub fn map_zeroed_user_page(&mut self, virtual_addr: u64)")
        code = function_body(PAGING, "pub fn map_zeroed_user_code_page(&mut self, virtual_addr: u64)")
        self.assertIn("PageFlags::USER_DATA", data)
        self.assertIn("PageFlags::USER_CODE", code)

    def test_destroy_is_unsafe_and_only_walks_user_half(self):
        self.assertIn("pub unsafe fn destroy(self)", PAGING)
        body = function_body(PAGING, "unsafe fn free_user_subtree(table_phys: u64, phys_offset: u64)")
        self.assertIn("0..(ENTRY_COUNT / 2)", body)
        self.assertIn("free_user_table_level", body)
        self.assertIn("frame::free_frame", PAGING)


if __name__ == "__main__":
    unittest.main(verbosity=2)
