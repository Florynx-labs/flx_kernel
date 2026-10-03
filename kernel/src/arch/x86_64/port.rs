//! x86 Port I/O abstractions via inline assembly.

use core::marker::PhantomData;

/// An x86 I/O port for reading and writing hardware registers.
#[derive(Debug, Clone, Copy)]
pub struct Port<T> {
    port: u16,
    _phantom: PhantomData<T>,
}

impl<T> Port<T> {
    /// Creates a new I/O port representation at the given 16-bit address.
    pub const fn new(port: u16) -> Self {
        Self {
            port,
            _phantom: PhantomData,
        }
    }
}

impl Port<u8> {
    /// Reads a byte from the I/O port (`in al, dx`).
    ///
    /// # Safety
    /// Port I/O can have arbitrary hardware side effects.
    #[inline]
    pub unsafe fn read(&self) -> u8 {
        let value: u8;
        core::arch::asm!(
            "in al, dx",
            in("dx") self.port,
            out("al") value,
            options(nomem, nostack, preserves_flags)
        );
        value
    }

    /// Writes a byte to the I/O port (`out dx, al`).
    ///
    /// # Safety
    /// Port I/O can have arbitrary hardware side effects.
    #[inline]
    pub unsafe fn write(&mut self, value: u8) {
        core::arch::asm!(
            "out dx, al",
            in("dx") self.port,
            in("al") value,
            options(nomem, nostack, preserves_flags)
        );
    }
}
