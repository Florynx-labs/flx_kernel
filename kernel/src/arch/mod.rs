//! Architecture-specific code for FLX Kernel.
//! Shared kernel services must depend on explicit architecture interfaces, not on
//! architecture register layouts or device-specific I/O.

pub mod interface;
pub mod x86_64;
pub mod aarch64;

#[cfg(target_arch = "x86_64")]
type ActiveCpu = x86_64::cpu::X86_64;
#[cfg(target_arch = "aarch64")]
type ActiveCpu = aarch64::AArch64;

#[inline]
pub fn local_interrupts_enabled() -> bool {
    <ActiveCpu as interface::CpuArchitecture>::interrupts_enabled()
}
#[inline]
pub fn disable_local_interrupts() {
    <ActiveCpu as interface::CpuArchitecture>::disable_interrupts()
}
#[inline]
pub fn enable_local_interrupts() {
    <ActiveCpu as interface::CpuArchitecture>::enable_interrupts()
}
pub fn nx_enabled() -> bool {
    <ActiveCpu as interface::CpuArchitecture>::nx_enabled()
}

pub unsafe fn activate_address_space(root_phys: u64) {
    // SAFETY: Forwarded from the caller to the selected architecture implementation.
    unsafe { <ActiveCpu as interface::CpuArchitecture>::activate_address_space(root_phys) }
}

pub fn current_address_space_root() -> u64 {
    <ActiveCpu as interface::CpuArchitecture>::current_address_space_root()
}

/// Invalidate one virtual address on the current CPU.
#[inline]
pub unsafe fn invalidate_tlb_page(address: u64) {
    // SAFETY: Forwarded from the caller to the selected architecture implementation.
    unsafe { <ActiveCpu as interface::CpuArchitecture>::invalidate_tlb_page(address) }
}

#[inline]
pub fn logical_cpu_id() -> u64 {
    <ActiveCpu as interface::CpuArchitecture>::logical_cpu_id()
}
#[inline]
pub fn halt_forever() -> ! {
    <ActiveCpu as interface::CpuArchitecture>::halt_forever()
}


/// Errors reported while installing the architecture-specific CPU exception environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpuInitError {
    Aarch64WrongExceptionLevel(u8),
}

/// Install the CPU tables/vectors required before enabling local interrupts.
///
/// The common kernel entry point must use this facade instead of reaching into
/// architecture-specific GDT/IDT/vector modules directly.
pub fn initialize_cpu_tables() -> Result<(), CpuInitError> {
    #[cfg(target_arch = "x86_64")]
    {
        x86_64::gdt::init();
        x86_64::idt::init();
        x86_64::syscall::init();
        x86_64::interrupts::trigger_breakpoint();
        Ok(())
    }

    #[cfg(target_arch = "aarch64")]
    {
        // SAFETY: this is called during privileged kernel initialization, before IRQs
        // are enabled. The architecture entry must provide a valid kernel stack.
        unsafe { aarch64::exceptions::install_el1_vectors() }
            .map_err(CpuInitError::Aarch64WrongExceptionLevel)
    }
}
