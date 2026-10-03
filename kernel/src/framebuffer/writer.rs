//! Framebuffer console writer with scrolling and color support.

use core::fmt;
use bootloader_api::info::{FrameBufferInfo, PixelFormat};
use super::font::{get_glyph, FONT_HEIGHT, FONT_WIDTH};

/// RGB 24-bit color representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    pub const BLACK: Color = Color::rgb(10, 15, 29);
    pub const WHITE: Color = Color::rgb(241, 245, 249);
    pub const CYAN: Color = Color::rgb(56, 189, 248);
    pub const GREEN: Color = Color::rgb(74, 222, 128);
    pub const YELLOW: Color = Color::rgb(250, 204, 21);
    pub const RED: Color = Color::rgb(248, 113, 113);
    pub const GRAY: Color = Color::rgb(148, 163, 184);
    pub const PURPLE: Color = Color::rgb(192, 132, 252);
}

/// A writer that renders text onto a linear graphics framebuffer.
pub struct FrameBufferWriter {
    buffer: &'static mut [u8],
    info: FrameBufferInfo,
    cursor_x: usize,
    cursor_y: usize,
    fg_color: Color,
    bg_color: Color,
}

impl FrameBufferWriter {
    /// Creates a new `FrameBufferWriter` wrapping the bootloader framebuffer.
    pub fn new(buffer: &'static mut [u8], info: FrameBufferInfo) -> Self {
        let mut writer = Self {
            buffer,
            info,
            cursor_x: 0,
            cursor_y: 0,
            fg_color: Color::WHITE,
            bg_color: Color::BLACK,
        };
        writer.clear();
        writer
    }

    /// Returns framebuffer width in pixels.
    fn width(&self) -> usize {
        self.info.width
    }

    /// Returns framebuffer height in pixels.
    fn height(&self) -> usize {
        self.info.height
    }

    /// Sets the text foreground color.
    pub fn set_fg_color(&mut self, color: Color) {
        self.fg_color = color;
    }

    /// Sets the text background color.
    pub fn set_bg_color(&mut self, color: Color) {
        self.bg_color = color;
    }

    /// Clears the entire framebuffer with the current background color.
    pub fn clear(&mut self) {
        let bg = self.bg_color;
        let width = self.width();
        let height = self.height();

        for y in 0..height {
            for x in 0..width {
                self.write_pixel(x, y, bg);
            }
        }
        self.cursor_x = 0;
        self.cursor_y = 0;
    }

    /// Sets a pixel at (x, y) to the specified color.
    pub fn write_pixel(&mut self, x: usize, y: usize, color: Color) {
        if x >= self.width() || y >= self.height() {
            return;
        }

        let stride = self.info.stride;
        let bpp = self.info.bytes_per_pixel;
        let byte_offset = (y * stride + x) * bpp;

        if byte_offset + bpp > self.buffer.len() {
            return;
        }

        match self.info.pixel_format {
            PixelFormat::Rgb => {
                self.buffer[byte_offset] = color.r;
                self.buffer[byte_offset + 1] = color.g;
                self.buffer[byte_offset + 2] = color.b;
            }
            PixelFormat::Bgr => {
                self.buffer[byte_offset] = color.b;
                self.buffer[byte_offset + 1] = color.g;
                self.buffer[byte_offset + 2] = color.r;
            }
            PixelFormat::U8 => {
                // Grayscale luminance fallback
                let gray = ((color.r as u32 * 77 + color.g as u32 * 150 + color.b as u32 * 29) >> 8) as u8;
                self.buffer[byte_offset] = gray;
            }
            _ => {
                // Default BGR fallback
                self.buffer[byte_offset] = color.b;
                self.buffer[byte_offset + 1] = color.g;
                self.buffer[byte_offset + 2] = color.r;
            }
        }
    }

    /// Draws a single character glyph at the current cursor position.
    pub fn write_char(&mut self, c: char) {
        match c {
            '\n' => self.newline(),
            '\r' => self.cursor_x = 0,
            '\t' => {
                for _ in 0..4 {
                    self.write_char(' ');
                }
            }
            _ => {
                if self.cursor_x + FONT_WIDTH > self.width() {
                    self.newline();
                }

                if self.cursor_y + FONT_HEIGHT > self.height() {
                    self.scroll();
                }

                let glyph = get_glyph(c);
                for (row, &byte) in glyph.iter().enumerate() {
                    for col in 0..FONT_WIDTH {
                        let bit = (byte >> (7 - col)) & 1;
                        let color = if bit != 0 { self.fg_color } else { self.bg_color };
                        self.write_pixel(self.cursor_x + col, self.cursor_y + row, color);
                    }
                }

                self.cursor_x += FONT_WIDTH;
            }
        }
    }

    /// Moves cursor to the start of the next line, scrolling if necessary.
    pub fn newline(&mut self) {
        self.cursor_x = 0;
        self.cursor_y += FONT_HEIGHT;
        if self.cursor_y + FONT_HEIGHT > self.height() {
            self.scroll();
        }
    }

    /// Scrolls the console buffer up by one text row (FONT_HEIGHT pixels).
    fn scroll(&mut self) {
        let stride = self.info.stride;
        let bpp = self.info.bytes_per_pixel;
        let row_bytes = stride * bpp;
        let scroll_bytes = FONT_HEIGHT * row_bytes;
        let height = self.height();
        let total_bytes = height * row_bytes;

        if scroll_bytes >= total_bytes || total_bytes > self.buffer.len() {
            self.cursor_y = 0;
            return;
        }

        // Shift all rows up by FONT_HEIGHT
        self.buffer.copy_within(scroll_bytes..total_bytes, 0);

        // Clear the newly exposed bottom lines
        let bg = self.bg_color;
        let clear_start_y = height.saturating_sub(FONT_HEIGHT);
        for y in clear_start_y..height {
            for x in 0..self.width() {
                self.write_pixel(x, y, bg);
            }
        }

        self.cursor_y = clear_start_y;
    }

    /// Writes an ASCII string to the console.
    pub fn write_string(&mut self, s: &str) {
        for c in s.chars() {
            self.write_char(c);
        }
    }

    /// Returns framebuffer resolution as (width, height).
    pub fn resolution(&self) -> (usize, usize) {
        (self.width(), self.height())
    }
}

impl fmt::Write for FrameBufferWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_string(s);
        Ok(())
    }
}
