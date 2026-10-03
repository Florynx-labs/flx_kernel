//! Framebuffer graphics and console subsystem for FLX Kernel.

pub mod font;
pub mod writer;

pub use writer::{Color, FrameBufferWriter};
use crate::sync::Spinlock;
use bootloader_api::info::FrameBufferInfo;
use core::fmt;

/// Global console framebuffer writer.
pub static FRAMEBUFFER: Spinlock<Option<FrameBufferWriter>> = Spinlock::new(None);

/// Initializes the global framebuffer console writer.
pub fn init(buffer: &'static mut [u8], info: FrameBufferInfo) {
    let writer = FrameBufferWriter::new(buffer, info);
    *FRAMEBUFFER.lock() = Some(writer);
}

/// Sets the global console text foreground color.
pub fn set_color(color: Color) {
    if let Some(ref mut writer) = *FRAMEBUFFER.lock() {
        writer.set_fg_color(color);
    }
}

/// Sets both foreground and background colors.
pub fn set_colors(fg: Color, bg: Color) {
    if let Some(ref mut writer) = *FRAMEBUFFER.lock() {
        writer.set_fg_color(fg);
        writer.set_bg_color(bg);
    }
}

/// Prints formatted arguments to the global framebuffer.
#[macro_export]
macro_rules! fb_print {
    ($($arg:tt)*) => {
        $crate::framebuffer::_print(core::format_args!($($arg)*))
    };
}

/// Prints formatted arguments followed by a newline to the global framebuffer.
#[macro_export]
macro_rules! fb_println {
    () => ($crate::fb_print!("\n"));
    ($($arg:tt)*) => ($crate::fb_print!("{}\n", core::format_args!($($arg)*)));
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    if let Some(ref mut writer) = *FRAMEBUFFER.lock() {
        let _ = writer.write_fmt(args);
    }
}
