use alloc::vec;
use alloc::vec::Vec;
use embedded_graphics::{pixelcolor::Rgb888, prelude::*};
#[allow(unused_imports)]
use log::{debug, error, info, trace};
use uefi::proto::console::gop::BltPixel;

pub struct Buffer {
    width: usize,
    height: usize,
    pixels: Vec<BltPixel>,
}

impl OriginDimensions for Buffer {
    fn size(&self) -> Size {
        Size::new(self.width as u32, self.height as u32)
    }
}

impl DrawTarget for Buffer {
    type Color = Rgb888;
    // `ExampleDisplay` uses a framebuffer and doesn't need to communicate with the display
    // controller to draw pixel, which means that drawing operations can never fail. To reflect
    // this the type `Infallible` was chosen as the `Error` type.
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(coord, color) in pixels.into_iter() {
            // Check if the pixel coordinates are out of bounds (negative or greater than
            // (63,63)). `DrawTarget` implementation are required to discard any out of bounds
            // pixels without returning an error or causing a panic.
            if coord.x >= 0
                && coord.y >= 0
                && coord.x < self.width as i32
                && coord.y < self.height as i32
            {
                // Calculate the index in the framebuffer.
                let pixel = self.pixel(coord.x as usize, coord.y as usize).unwrap();
                pixel.red = color.r();
                pixel.green = color.g();
                pixel.blue = color.b();
            }
        }

        Ok(())
    }
}

impl Buffer {
    /// Create a new `Buffer`.
    pub fn new(width: usize, height: usize) -> Self {
        Buffer {
            width,
            height,
            pixels: vec![BltPixel::new(0, 0, 0); width * height],
        }
    }

    /// Get a single pixel.
    pub fn pixel(&mut self, x: usize, y: usize) -> Option<&mut BltPixel> {
        self.pixels.get_mut(y * self.width + x)
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    /// All pixels, row by row
    pub fn pixels(&self) -> &[BltPixel] {
        &self.pixels
    }
}
