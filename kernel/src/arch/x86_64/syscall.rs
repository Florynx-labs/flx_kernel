//! Minimal, explicit int 0x80 syscall ABI for initial Ring 3 bring-up.
//!
//! Initial int 0x80 ABI for the single-task Ring 3 bring-up prototype.
//!
//! SYS_WRITE copies bytes through the active AddressSpace's validated physical mapping. The
//! ABI is intentionally small and not yet a stable userspace contract.

use super::interrupts::{self, InterruptFrame};

pub const SYSCALL_VECTOR: u8 = 0x80;
pub const SYS_EXIT: u64 = 0;
pub const SYS_WRITE: u64 = 1;
pub const SYS_GETPID: u64 = 2;

const ENOSYS: i64 = -38;
const EBADF: i64 = -9;
const EFAULT: i64 = -14;
const EINVAL: i64 = -22;
const MAX_WRITE_BYTES: usize = 1024;

static INITIAL_TASK_EXITED: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

/// Installs the user-callable interrupt gate's kernel-side dispatcher.
pub fn init() {
    interrupts::register_handler(SYSCALL_VECTOR, dispatch);
}

/// Returns whether the initial task has requested exit.
pub fn initial_task_exited() -> bool {
    INITIAL_TASK_EXITED.load(core::sync::atomic::Ordering::Acquire)
}

fn dispatch(frame: &mut InterruptFrame) {
    match frame.rax {
        SYS_EXIT => {
            INITIAL_TASK_EXITED.store(true, core::sync::atomic::Ordering::Release);
            // No scheduler exists yet, so report completion and return to the caller.
            frame.rax = 0;
        }
        SYS_GETPID => {
            // PID 1 is reserved for the single bring-up task.
            frame.rax = 1;
        }
        SYS_WRITE => {
            // ABI: RDI=fd (1 stdout or 2 stderr), RSI=user pointer, RDX=byte length.
            if frame.rdi != 1 && frame.rdi != 2 {
                frame.rax = EBADF as u64;
                return;
            }
            let Ok(length) = usize::try_from(frame.rdx) else {
                frame.rax = EINVAL as u64;
                return;
            };
            if length > MAX_WRITE_BYTES {
                frame.rax = EINVAL as u64;
                return;
            }
            let mut bytes = [0u8; MAX_WRITE_BYTES];
            if crate::mm::process::copy_from_current_user(frame.rsi, &mut bytes[..length]).is_err() {
                frame.rax = EFAULT as u64;
                return;
            }
            // Raw bytes are emitted without assuming that userspace supplied valid UTF-8.
            let mut serial = crate::drivers::serial::SERIAL1.lock();
            for byte in bytes[..length].iter().copied() {
                serial.send(byte);
            }
            frame.rax = length as u64;
        }
        _ => frame.rax = ENOSYS as u64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(number: u64) -> InterruptFrame {
        InterruptFrame {
            r15: 0, r14: 0, r13: 0, r12: 0, r11: 0, r10: 0, r9: 0, r8: 0,
            rdi: 0, rsi: 0, rbp: 0, rdx: 0, rcx: 0, rbx: 0, rax: number,
            vector: SYSCALL_VECTOR as u64, error_code: 0, rip: 0x400000, cs: 0x23,
            rflags: 0x202, rsp: 0x7fff_ffff_fff0, ss: 0x1b,
        }
    }

    #[test]
    fn getpid_returns_initial_pid() {
        let mut f = frame(SYS_GETPID);
        dispatch(&mut f);
        assert_eq!(f.rax, 1);
    }

    #[test]
    fn unknown_syscall_returns_enosys() {
        let mut f = frame(u64::MAX);
        dispatch(&mut f);
        assert_eq!(f.rax as i64, ENOSYS);
    }

    #[test]
    fn write_rejects_invalid_file_descriptor() {
        let mut f = frame(SYS_WRITE);
        dispatch(&mut f);
        assert_eq!(f.rax as i64, EBADF);
    }

    #[test]
    fn write_rejects_excessive_length() {
        let mut f = frame(SYS_WRITE);
        f.rdi = 1;
        f.rdx = (MAX_WRITE_BYTES + 1) as u64;
        dispatch(&mut f);
        assert_eq!(f.rax as i64, EINVAL);
    }
}
