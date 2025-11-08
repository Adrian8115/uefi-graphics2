#![no_std]

extern crate alloc;
use alloc::vec::Vec;
use embedded_graphics::pixelcolor::RgbColor;
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{OriginDimensions, Size},
    pixelcolor::{IntoStorage, Rgb888},
    prelude::Point,
    primitives::Rectangle,
    Pixel,
};
use uefi::proto::console::gop::{FrameBuffer, ModeInfo};

pub use crate::error::UefiDisplayError;
pub mod error;

/// A double‑buffered UEFI display that implements `embedded_graphics::DrawTarget<Rgb888>`.
#[derive(Debug)]
pub struct UefiDisplay {
    /// Raw UEFI frame buffer pointer
    fb_ptr: *mut u8,
    /// In‑memory double buffer (fully owned)
    buffer: Vec<u8>,
    stride: u32,
    size: (u32, u32),
}

impl UefiDisplay {
    /// Create a new, cleared-to-black `UefiDisplay`.
    pub fn new(
        mut frame_buffer: FrameBuffer,
        mode_info: ModeInfo,
    ) -> Result<Self, UefiDisplayError> {
        let (width, height) = (
            mode_info.resolution().0 as u32,
            mode_info.resolution().1 as u32,
        );
        let stride = mode_info.stride() as u32;
        let buf_len = stride
            .checked_mul(height)
            .and_then(|p| p.checked_mul(4))
            .ok_or(UefiDisplayError::InvalidResolution)?;

        // Allocate zeroed buffer to avoid UB
        let mut buffer = Vec::new();
        buffer.resize(buf_len as usize, 0);

        let mut display = UefiDisplay {
            fb_ptr: frame_buffer.as_mut_ptr(),
            buffer,
            stride,
            size: (width, height),
        };

        // Fill to black
        display.fill_solid(
            &Rectangle::new(Point::zero(), Size::new(width, height)),
            Rgb888::BLACK,
        )?;
        display.flush();

        Ok(display)
    }

    /// Copies the in‑memory buffer out to the UEFI frame buffer.
    pub fn flush(&self) {
        // SAFETY: We know both buffers are at least buffer.len() bytes long.
        unsafe {
            core::ptr::copy_nonoverlapping(self.buffer.as_ptr(), self.fb_ptr, self.buffer.len());
        }
    }
}

impl OriginDimensions for UefiDisplay {
    fn size(&self) -> Size {
        Size::new(self.size.0, self.size.1)
    }
}

impl DrawTarget for UefiDisplay {
    type Color = Rgb888;
    type Error = UefiDisplayError;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        let (width, _height) = self.size;
        let stride = self.stride as usize;
        let buf = &mut self.buffer;

        for Pixel(Point { x, y }, color) in pixels.into_iter() {
            // bounds‑check
            if x < 0 || y < 0 {
                continue;
            }
            let (x, y): (usize, usize) = (x as usize, y as usize);
            if x >= width as usize {
                continue;
            }

            // compute byte index safely
            let idx = y
                .checked_mul(stride)
                .and_then(|row| row.checked_add(x))
                .and_then(|pix| pix.checked_mul(4))
                .ok_or(UefiDisplayError::OutOfBounds)?;

            let pixel_val: u32 = color.into_storage();
            let pixel_bytes = pixel_val.to_le_bytes();
            buf[idx..idx + 4].copy_from_slice(&pixel_bytes);
        }
        Ok(())
    }
}
