//! First-step Ring 3 entry and user-stack preparation.
//!
//! This is a single-core bring-up primitive, not a process scheduler. A dedicated kernel
//! interrupt stack is used for the first user task until per-thread kernel stacks exist.

use core::arch::asm;

use super::gdt;
use crate::mm::paging::{AddressSpace, PagingError};

pub const USER_STACK_PAGES: usize = 4;
pub const USER_STACK_TOP: u64 = 0x0000_7fff_ffff_f000;
const KERNEL_INTERRUPT_STACK_SIZE: usize = 16 * 1024;

#[repr(C, align(16))]
struct KernelInterruptStack([u8; KERNEL_INTERRUPT_STACK_SIZE]);

static mut USER_TASK_KERNEL_STACK: KernelInterruptStack =
    KernelInterruptStack([0; KERNEL_INTERRUPT_STACK_SIZE]);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserModeError {
    Paging(PagingError),
    InvalidEntry,
}

/// Ensures the dedicated Ring 3 transition/interrupt stack is mapped in a process root.
///
/// The stack is statically allocated in the kernel image. Explicitly preserve each leaf page
/// instead of relying only on inherited PML4 entries from the bootloader's page tables.
pub fn prepare_kernel_stack(space: &mut AddressSpace) -> Result<(), UserModeError> {
    let base = core::ptr::addr_of_mut!(USER_TASK_KERNEL_STACK) as u64;
    let first = base & !(4096 - 1);
    let end = base.checked_add(KERNEL_INTERRUPT_STACK_SIZE as u64)
        .ok_or(UserModeError::InvalidEntry)?;
    let limit = end.checked_add(4095).ok_or(UserModeError::InvalidEntry)? & !(4096 - 1);
    let mut page = first;
    while page < limit {
        space.ensure_kernel_mapping(page).map_err(UserModeError::Paging)?;
        page += 4096;
    }
    Ok(())
}

/// Allocates a small, zero-filled user stack in the supplied address space.
pub fn prepare_user_stack(space: &mut AddressSpace) -> Result<u64, UserModeError> {
    let first = USER_STACK_TOP
        .checked_sub((USER_STACK_PAGES as u64) * 4096)
        .ok_or(UserModeError::InvalidEntry)?;
    let mut page = first;
    while page < USER_STACK_TOP {
        if let Err(error) = space.map_zeroed_user_page(page) {
            let mut rollback = first;
            while rollback < page {
                let _ = space.unmap_user_page(rollback);
                rollback += 4096;
            }
            return Err(UserModeError::Paging(error));
        }
        page += 4096;
    }
    Ok((USER_STACK_TOP - 16) & !0xF)
}

/// Enters Ring 3 at an executable user entry point with a prepared user stack.
///
/// # Safety
/// The entry must be executable user memory and the stack must point into writable user
/// memory in the intended address space. The caller must ensure the address space is active,
/// interrupt handlers are initialized, and the kernel stack is mapped supervisor-only.
/// This prototype supports one user task on the boot CPU only.
pub unsafe fn enter_user_mode(entry: u64, user_stack: u64) -> ! {
    if entry < 0x1_0000 || entry >= 0x0000_8000_0000_0000
        || user_stack < 0x1_0000 || user_stack >= 0x0000_8000_0000_0000
        || user_stack & 0xF != 0
    {
        crate::kerror!("Refusing invalid Ring 3 entry/stack");
        crate::arch::x86_64::cpu::halt_loop();
    }

    // SAFETY: the static stack is writable and has static lifetime; single-task prototype.
    let kernel_stack_top = unsafe {
        core::ptr::addr_of_mut!(USER_TASK_KERNEL_STACK) as u64 + KERNEL_INTERRUPT_STACK_SIZE as u64
    };
    // SAFETY: stack is valid, mapped, aligned, and remains alive permanently.
    unsafe { gdt::set_kernel_stack_top(kernel_stack_top); }

    const USER_RFLAGS: u64 = 0x202;
    unsafe {
        asm!(
            // CR3 was switched before entering this function. The boot stack may not be
            // mapped in the new address space, so move to the dedicated kernel stack before
            // the first push; otherwise preparing the IRETQ frame can page-fault in Ring 0.
            "mov rsp, {kernel_stack_top}",
            "push {user_ss}",
            "push {user_rsp}",
            "push {rflags}",
            "push {user_cs}",
            "push {entry}",
            "iretq",
            kernel_stack_top = in(reg) kernel_stack_top,
            user_ss = in(reg) gdt::USER_DATA_SELECTOR as u64,
            user_rsp = in(reg) user_stack,
            rflags = in(reg) USER_RFLAGS,
            user_cs = in(reg) gdt::USER_CODE_SELECTOR as u64,
            entry = in(reg) entry,
            options(noreturn)
        );
    }
}
