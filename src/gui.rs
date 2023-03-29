use alloc::vec;
use alloc::vec::Vec;
#[allow(unused_imports)]
use log::{debug, error, info};
#[allow(unused_imports)]
use uefi::prelude::*;
use uefi::proto::console::gop::{BltOp, BltPixel, BltRegion, GraphicsOutput};
use uefi::table::boot::BootServices;
use uefi::Result;

use embedded_graphics::{image::Image, pixelcolor::Rgb565, prelude::*};
use tinybmp::Bmp;

struct Buffer {
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
    type Color = Rgb565;
    // `ExampleDisplay` uses a framebuffer and doesn't need to communicate with the display
    // controller to draw pixel, which means that drawing operations can never fail. To reflect
    // this the type `Infallible` was chosen as the `Error` type.
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> core::result::Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(coord, color) in pixels.into_iter() {
            // Check if the pixel coordinates are out of bounds (negative or greater than
            // (63,63)). `DrawTarget` implementation are required to discard any out of bounds
            // pixels without returning an error or causing a panic.
            if coord.x < self.width as i32 && coord.y < self.height as i32 {
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
    fn new(width: usize, height: usize) -> Self {
        Buffer {
            width,
            height,
            pixels: vec![BltPixel::new(0, 0, 0); width * height],
        }
    }

    /// Get a single pixel.
    fn pixel(&mut self, x: usize, y: usize) -> Option<&mut BltPixel> {
        self.pixels.get_mut(y * self.width + x)
    }

    /// Blit the buffer to the framebuffer.
    fn blit(&self, gop: &mut GraphicsOutput) -> Result {
        gop.blt(BltOp::BufferToVideo {
            buffer: &self.pixels,
            src: BltRegion::Full,
            dest: (0, 0),
            dims: (self.width, self.height),
        })
    }
}

pub fn draw_background(bs: &BootServices) -> Result {
    // Open graphics output protocol.
    let gop_handle = bs.get_handle_for_protocol::<GraphicsOutput>()?;
    // open_protocol_exclusive will steal it from anyone else, meaning shell prints won't work anymore
    let mut gop = bs.open_protocol_exclusive::<GraphicsOutput>(gop_handle)?;

    let mode = gop.current_mode_info();
    let (width, height) = mode.resolution();
    info!("Current Resolution {}x{}", width, height);

    // Create a buffer to draw into.
    let (width, height) = gop.current_mode_info().resolution();
    let mut buffer = Buffer::new(width, height);

    // Initialize the buffer with a simple gradient background.
    for y in 0..height {
        let r = ((y as f32) / ((height - 1) as f32)) * 255.0;
        for x in 0..width {
            let g = ((x as f32) / ((width - 1) as f32)) * 255.0;
            let pixel = buffer.pixel(x, y).unwrap();
            pixel.red = r as u8;
            pixel.green = g as u8;
            pixel.blue = 255;
        }
    }

    let bmp: Bmp<Rgb565> = Bmp::from_slice(include_bytes!(
        "../../led_matrix_fw/fl16-inputmodules/assets/logo.bmp"
    ))
    .unwrap();
    let image = Image::new(&bmp, Point::new(100, 100));
    image.draw(&mut buffer).unwrap();

    // Draw the buffer to the screen.
    buffer.blit(&mut gop)?;
    loop {
        bs.stall(10_000_000);
    }
}
