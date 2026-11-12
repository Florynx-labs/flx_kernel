//! End-to-end preparation and launch of a bounded static ELF64 user process.
//!
//! The loader builds the address space while it is inactive. Launch is an explicit unsafe
//! operation: it switches CR3 and transfers control to Ring 3, and currently has no scheduler
//! or return-to-kernel process teardown path.

use crate::arch::x86_64::usermode::{self, UserModeError};
use crate::mm::elf_loader::{self, ElfLoadError};
use crate::mm::paging::{AddressSpace, PagingError};
use core::mem::{ManuallyDrop, MaybeUninit};
use core::sync::atomic::{AtomicPtr, Ordering};

// Single-task bring-up context. The owner lives in kernel .bss, not on the boot stack:
// the process page tables intentionally do not inherit the bootloader stack mapping.
static mut CURRENT_ADDRESS_SPACE_STORAGE: MaybeUninit<AddressSpace> = MaybeUninit::uninit();
static CURRENT_ADDRESS_SPACE: AtomicPtr<AddressSpace> = AtomicPtr::new(core::ptr::null_mut());

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessError {
    AddressSpace(PagingError),
    Elf(ElfLoadError),
    UserMode(UserModeError),
}

/// A prepared, single-task userspace image.
///
/// The address space owns all mapped ELF and stack pages. It must remain inactive while being
/// constructed, and must not be destroyed while active on any CPU.
pub struct UserProcess {
    address_space: AddressSpace,
    entry: u64,
    user_stack: u64,
}

impl UserProcess {
    /// Creates an isolated address space, loads a static ELF64 image, and prepares its user stack.
    ///
    /// phys_offset must be the valid physical-memory mapping supplied by the UEFI bootloader.
    /// The image is copied into owned user pages; the caller does not need to retain bytes.
    ///
    /// # Safety
    /// The physical-memory offset must be valid for the lifetime of the process, and the frame
    /// allocator must already be initialized. This routine must run with the new address space
    /// inactive, as required by the underlying ELF loader. all ust be documented if you Don't do it
    /// we can't advance note by Asuno sorry i'm newbie in Rust 

    pub unsafe fn prepare(bytes: &[u8], phys_offset: u64) -> Result<Self, ProcessError> {
        // SAFETY: forwarded contract guarantees the bootloader physical mapping is valid.
    
        let mut address_space = unsafe { AddressSpace::new(phys_offset) }
            .map_err(ProcessError::AddressSpace)?;

        if let Err(error) = usermode::prepare_kernel_stack(&mut address_space) {
            // SAFETY: this newly created address space has not been activated.
            let _ = unsafe { address_space.destroy() };
            return Err(ProcessError::UserMode(error));
        }

        let entry = match elf_loader::load_elf(&mut address_space, bytes) {
            Ok(entry) => entry,
            Err(error) => {
                // SAFETY: this newly created address space has not been activated.
                let _ = unsafe { address_space.destroy() };
                return Err(ProcessError::Elf(error));
            }
        };

        let user_stack = match usermode::prepare_user_stack(&mut address_space) {
            Ok(stack) => stack,
            Err(error) => {
                // SAFETY: this newly created address space has not been activated.
                let _ = unsafe { address_space.destroy() };
                return Err(ProcessError::UserMode(error));
            }
        };

        Ok(Self { address_space, entry, user_stack })
    }

    pub fn entry(&self) -> u64 {
        self.entry
    }

    pub fn user_stack(&self) -> u64 {
        self.user_stack
    }


    pub fn mapped_user_pages(&self) -> usize {
        self.address_space.mapped_user_pages()
    }
    
    /// Switches into this process and transfers control to its ELF entry point in Ring 3.
    ///
    /// This is a one-way bring-up operation: there is no scheduler, process reaper, or supported
    /// return path yet. Do not call it until IDT/syscalls are initialized and the kernel stack is
    /// mapped supervisor-only in the shared kernel half of the address space.
    ///
    /// # Safety
    /// The process must be fully prepared, the frame allocator and interrupt subsystem must be
    /// initialized, and no other CPU may use or destroy this address space. This function never
    /// returns.
    pub unsafe fn enter(self) -> ! {
        // Keep the address-space owner at a stable kernel-image address. A pointer to
        // `self.address_space` would point into the boot stack, which is deliberately
        // absent from the isolated process root; the first syscall would then fault.
        let process = ManuallyDrop::new(self);
        let entry = process.entry;
        let user_stack = process.user_stack;
        // SAFETY: ManuallyDrop prevents the old stack-resident UserProcess from dropping
        // the moved address space. This one-shot static slot is initialized exactly once
        // by the single-task bring-up path and remains alive for the kernel lifetime.
        let address_space = unsafe { core::ptr::read(&process.address_space) };
        let storage = core::ptr::addr_of_mut!(CURRENT_ADDRESS_SPACE_STORAGE).cast::<AddressSpace>();
        unsafe { storage.write(address_space); }
        CURRENT_ADDRESS_SPACE.store(storage, Ordering::Release);

        let root_phys = unsafe { (*storage).root_physical_address() };
        let phys_offset = unsafe { (*storage).physical_memory_offset() };
        crate::kinfo!(
            "Ring 3 address space: root={:#x}, phys_offset={:#x}, physmap PML4 index={}, context={:#x}",
            root_phys,
            phys_offset,
            (phys_offset >> 39) & 0x1ff,
            storage as u64
        );
        // Once CR3 changes, diagnostics must not write to the bootloader framebuffer mapping:
        // it lives in a lower-half slot intentionally not inherited by this process root.
        crate::logging::set_serial_only();
        // SAFETY: entry, user stack, root, and dedicated kernel stack were prepared above.
        unsafe { usermode::enter_user_mode(root_phys, entry, user_stack) }
    }
}


/// Copies a bounded byte range from the currently running user process into kernel memory.
///
/// This is available only after UserProcess::enter publishes the single active process context.
/// The current bring-up design is single-task/single-core; a scheduler must replace this global
/// context before multiple processes or CPUs can issue syscalls concurrently.
pub fn copy_from_current_user(
    user_address: u64,
    destination: &mut [u8],
) -> Result<(), PagingError> {
    let pointer = CURRENT_ADDRESS_SPACE.load(Ordering::Acquire);
    if pointer.is_null() {
        return Err(PagingError::NotMapped);
    }
    // SAFETY: the pointer is published only from UserProcess::enter, whose ! return type keeps
    // the owning process alive indefinitely in this prototype. No process teardown exists yet.
    let address_space = unsafe { &*pointer };
    address_space.read_user_bytes(user_address, destination)
}
