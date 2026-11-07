#!/usr/bin/env python3
"""Normalize a runtime kernel RIP to its linked ELF virtual address."""

from __future__ import annotations

import argparse


def normalize_rip(runtime_rip: int, relocation_offset: int) -> int:
    if runtime_rip < 0 or relocation_offset < 0:
        raise ValueError("addresses must be non-negative")
    if runtime_rip < relocation_offset:
        raise ValueError("runtime RIP is below the kernel relocation base")
    return runtime_rip - relocation_offset


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("runtime_rip", type=lambda value: int(value, 0))
    parser.add_argument("relocation_offset", type=lambda value: int(value, 0))
    args = parser.parse_args()
    try:
        print(f"0x{normalize_rip(args.runtime_rip, args.relocation_offset):x}")
    except ValueError as error:
        parser.error(str(error))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
