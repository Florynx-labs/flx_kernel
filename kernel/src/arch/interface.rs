//! Architecture-neutral CPU contract used by common kernel services.
//!
//! Implementations must keep privileged instructions and register layouts inside
//! `arch/<architecture>`. A method is only exposed when the target has a real implementation.

/// Operations required by architecture-independent kernel code.
pub trait CpuArchitecture {
    /// Stable architecture identifier for diagnostics.
    const NAME: &'static str;

    /// Mask local maskable interrupts on this logical CPU.
    fn disable_interrupts();

    /// Unmask local maskable interrupts on this logical CPU.
    fn enable_interrupts();

    /// Return whether local maskable interrupts are currently enabled.
    fn interrupts_enabled() -> bool;

    /// Return the hardware identifier of the current logical CPU.
    fn logical_cpu_id() -> u64;

    /// Stop execution permanently on the current CPU.
    fn halt_forever() -> !;

    /// Install a new root page-table address space for the current CPU.
    ///
    /// # Safety
    /// `root_phys` must be aligned and refer to a valid translation-table root configured
    /// for the active translation regime. The caller must ensure the kernel remains mapped.
    unsafe fn activate_address_space(root_phys: u64);

    /// Invalidate all non-global address translations on the current CPU.
    ///
    /// # Safety
    /// The caller must coordinate address-space mutation and shootdowns on other CPUs.
    unsafe fn invalidate_tlb_all();
}
