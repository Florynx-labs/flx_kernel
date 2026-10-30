//! Cooperative x86_64 kernel-task context switching.
//!
//! This backend saves the System V callee-saved general-purpose registers and the stack
//! pointer. It is the foundation for kernel-thread switching; interrupt-frame preemption,
//! FPU/SIMD state, address-space switching, and scheduler integration are separate steps.

use core::arch::global_asm;

const INITIAL_FRAME_WORDS: usize = 8;
const INITIAL_FRAME_BYTES: usize = INITIAL_FRAME_WORDS * core::mem::size_of::<usize>();

/// Minimal cooperative kernel-task context.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskContext {
    stack_pointer: usize,
}

impl TaskContext {
    /// Creates a context that has not been initialized with a task stack.
    pub const fn empty() -> Self {
        Self { stack_pointer: 0 }
    }

    /// Returns whether this context has a stack frame ready for the switch routine.
    pub const fn is_initialized(&self) -> bool {
        self.stack_pointer != 0
    }

    /// Builds the first stack frame for a new kernel task.
    ///
    /// The entry function must not return. The caller must keep the backing stack memory alive
    /// and exclusively owned by this task until the context is no longer runnable.
    ///
    /// # Safety
    /// The stack must be dedicated to this task and must not be accessed or freed while the
    /// task can execute. The entry must be a valid kernel-mode function that never returns.
    pub unsafe fn initialize(
        &mut self,
        stack: &mut [u8],
        entry: extern "C" fn() -> !,
    ) -> Result<(), ContextError> {
        if stack.len() < INITIAL_FRAME_BYTES + 16 {
            return Err(ContextError::StackTooSmall);
        }

        let base = stack.as_mut_ptr() as usize;
        let end = base.checked_add(stack.len()).ok_or(ContextError::InvalidStack)?;
        let top = end & !0xF;
        let frame_address = top
            .checked_sub(INITIAL_FRAME_BYTES)
            .ok_or(ContextError::InvalidStack)?;
        if frame_address < base {
            return Err(ContextError::StackTooSmall);
        }

        // Six zeroed callee-saved registers are popped by the assembly routine. The return
        // instruction then transfers to entry. The final word preserves the System V entry
        // stack alignment (RSP mod 16 = 8).
        let frame = frame_address as *mut usize;
        unsafe {
            for index in 0..6 {
                frame.add(index).write(0);
            }
            frame.add(6).write(entry as usize);
            frame.add(7).write(0);
        }
        self.stack_pointer = frame_address;
        Ok(())
    }

    /// Saves the current cooperative context and resumes next.
    ///
    /// This does not switch CR3, update the TSS kernel stack, or save caller-clobbered/FPU
    /// state. Use only at a normal Rust/C ABI call boundary between kernel tasks sharing one
    /// address space and execution environment.
    ///
    /// # Safety
    /// next must be initialized and its stack must remain mapped and writable. Both contexts
    /// must belong to the same CPU and address space. Callers must serialize access and must
    /// not invoke this cooperative primitive from interrupt context.
    pub unsafe fn switch_to(&mut self, next: &TaskContext) {
        if !next.is_initialized() || self.stack_pointer == next.stack_pointer {
            return;
        }
        // SAFETY: upheld by the caller; assembly saves current RSP and loads the prepared RSP.
        unsafe {
            flx_context_switch(
                core::ptr::addr_of_mut!(self.stack_pointer),
                core::ptr::addr_of!(next.stack_pointer),
            );
        }
    }
}

impl Default for TaskContext {
    fn default() -> Self {
        Self::empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextError {
    StackTooSmall,
    InvalidStack,
}

extern "C" {
    fn flx_context_switch(old_stack_pointer: *mut usize, new_stack_pointer: *const usize);
}

global_asm!(
    r#"
    .text
    .balign 16
    .global flx_context_switch
    .type flx_context_switch, @function
flx_context_switch:
    push rbp
    push rbx
    push r12
    push r13
    push r14
    push r15
    mov [rdi], rsp
    mov rsp, [rsi]
    pop r15
    pop r14
    pop r13
    pop r12
    pop rbx
    pop rbp
    ret
    .size flx_context_switch, .-flx_context_switch
    "#,
);
