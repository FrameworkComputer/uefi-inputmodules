use alloc::vec;
use alloc::vec::Vec;
use embedded_graphics::primitives::{PrimitiveStyle, PrimitiveStyleBuilder, Rectangle};
#[allow(unused_imports)]
use log::{debug, error, info};
use uefi::proto::console::gop::{BltOp, BltPixel, BltRegion, GraphicsOutput};
use uefi::proto::console::text::{Input, Key, ScanCode};
use uefi::proto::device_path::text::{AllowShortcuts, DevicePathToText, DisplayOnly};
use uefi::proto::device_path::DevicePath;
use uefi::table::boot::{BootServices, ScopedProtocol};
use uefi::table::boot::{OpenProtocolAttributes, OpenProtocolParams, SearchType};
use uefi::Identify;
use uefi::Result;
#[allow(unused_imports)]
use uefi::{prelude::*, Char16};

use embedded_graphics::{
    image::Image,
    mono_font::{ascii::FONT_9X15, MonoTextStyle},
    pixelcolor::Rgb888,
    prelude::*,
    text::Text,
};
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
    type Color = Rgb888;
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

// Just experimenting
pub fn _draw_console(st: &mut SystemTable<Boot>) -> Result {
    let stdout = st.stdout();
    stdout.reset(false).unwrap();
    stdout.set_cursor_position(0, 0).unwrap();
    stdout.output_string(cstr16!("string")).unwrap();
    stdout.set_cursor_position(10, 10).unwrap();
    stdout.output_string(cstr16!("foo")).unwrap();
    stdout.set_cursor_position(20, 20).unwrap();
    stdout.output_string(cstr16!("bar")).unwrap();
    Ok(())
}

pub fn draw_background(bs: &BootServices, _st: &SystemTable<Boot>) -> Result {
    //// Open graphics output protocol.
    //let gop_handle = bs.get_handle_for_protocol::<GraphicsOutput>()?;
    //// open_protocol_exclusive will steal it from anyone else, meaning shell prints won't work anymore
    //let mut gop = bs.open_protocol_exclusive::<GraphicsOutput>(gop_handle)?;

    //let mut gop = unsafe {
    //    bs.open_protocol::<GraphicsOutput>(
    //        OpenProtocolParams {
    //            handle: *st.stdout_handle(),
    //            agent: bs.image_handle(),
    //            controller: None,
    //        },
    //        OpenProtocolAttributes::GetProtocol,
    //    )
    //    .expect("Failed to open GraphicsOutput handle")
    //};

    let gop_handles = if let Ok(gop_handles) =
        bs.locate_handle_buffer(SearchType::ByProtocol(&GraphicsOutput::GUID))
    {
        gop_handles
    } else {
        info!("GraphicsOutput protocol is missing");
        return Ok(());
    };

    let mut gop = if let Some(handle) = gop_handles.iter().next() {
        unsafe {
            bs.open_protocol::<GraphicsOutput>(
                OpenProtocolParams {
                    handle: *handle,
                    agent: bs.image_handle(),
                    controller: None,
                },
                OpenProtocolAttributes::GetProtocol,
            )
            .expect("Failed to open GraphicsOutput handle")
        }
    } else {
        info!("No GOP handles found");
        return Ok(());
    };

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

    let bmp: Bmp<Rgb888> = Bmp::from_slice(include_bytes!(
        "../../led_matrix_fw/fl16-inputmodules/assets/logo.bmp"
    ))
    .unwrap();
    let image = Image::new(&bmp, Point::new(100, 100));
    image.draw(&mut buffer).unwrap();

    // Have to export in GIMP as RGB888
    let background_bmp: Bmp<Rgb888> = Bmp::from_slice(include_bytes!(
        "/home/zoid/Downloads/framework-laptop-16-input-module-topdown.bmp"
    ))
    .unwrap();
    let image = Image::new(&background_bmp, Point::new(0, 0));
    image.draw(&mut buffer).unwrap();

    let blank_style: PrimitiveStyle<Rgb888> = PrimitiveStyleBuilder::new()
        .fill_color(Rgb888::CSS_GRAY)
        .build();

    let left_c1_rect: Rectangle =
        Rectangle::with_corners(Point::new(255, 360), Point::new(310, 545));
    let middle_a1_rect: Rectangle =
        Rectangle::with_corners(Point::new(311, 360), Point::new(775, 545));
    let right_c1_rect: Rectangle =
        Rectangle::with_corners(Point::new(776, 360), Point::new(830, 545));

    left_c1_rect
        .into_styled(blank_style)
        .draw(&mut buffer)
        .unwrap();

    middle_a1_rect
        .into_styled(blank_style)
        .draw(&mut buffer)
        .unwrap();

    right_c1_rect
        .into_styled(blank_style)
        .draw(&mut buffer)
        .unwrap();

    let text = Text::new(
        "Framework Laptop 16 Input Deck - Missing Modules",
        Point::new(250, 30),
        MonoTextStyle::new(&FONT_9X15, Rgb888::BLACK),
    );

    text.draw(&mut buffer).unwrap();

    // Draw the buffer to the screen.
    buffer.blit(&mut gop)?;

    let input_handle = bs.get_handle_for_protocol::<Input>().unwrap();
    let mut input = bs.open_protocol_exclusive::<Input>(input_handle).unwrap();

    let mut x = 0;
    let mut y = 0;
    let mut game = false;
    let one_key = Char16::try_from('1').unwrap();
    let two_key = Char16::try_from('2').unwrap();
    let three_key = Char16::try_from('3').unwrap();
    let left_c1_sprite = background_bmp.sub_image(&left_c1_rect);
    let middle_a1_sprite = background_bmp.sub_image(&middle_a1_rect);
    let right_c1_sprite = background_bmp.sub_image(&right_c1_rect);
    let mut left_c1_state = false;
    let mut middle_a1_state = false;
    let mut right_c1_state = false;
    loop {
        // Pause until a keyboard event occurs.
        let mut events = unsafe { [input.wait_for_key_event().unsafe_clone()] };
        bs.wait_for_event(&mut events).discard_errdata().unwrap();

        //let u_key = Char16::try_from('u').unwrap();
        match input.read_key().unwrap() {
            // Example of handling a printable key: print a message when
            // the 'u' key is pressed.
            //Some(Key::Printable(key)) if key == u_key => {
            //    info!("the 'u' key was pressed");
            //}
            Some(Key::Printable(key)) if key == one_key => {
                left_c1_state = !left_c1_state;
                if left_c1_state {
                    Image::new(&left_c1_sprite, left_c1_rect.top_left)
                        .draw(&mut buffer)
                        .unwrap();
                } else {
                    left_c1_rect
                        .into_styled(blank_style)
                        .draw(&mut buffer)
                        .unwrap();
                }
            }
            Some(Key::Printable(key)) if key == two_key => {
                middle_a1_state = !middle_a1_state;
                if middle_a1_state {
                    Image::new(&middle_a1_sprite, middle_a1_rect.top_left)
                        .draw(&mut buffer)
                        .unwrap();
                } else {
                    middle_a1_rect
                        .into_styled(blank_style)
                        .draw(&mut buffer)
                        .unwrap();
                }
            }
            Some(Key::Printable(key)) if key == three_key => {
                right_c1_state = !right_c1_state;
                if right_c1_state {
                    Image::new(&right_c1_sprite, right_c1_rect.top_left)
                        .draw(&mut buffer)
                        .unwrap();
                } else {
                    right_c1_rect
                        .into_styled(blank_style)
                        .draw(&mut buffer)
                        .unwrap();
                }
            }
            Some(Key::Special(ScanCode::LEFT)) => {
                y -= 10;
                game = true;
            }
            Some(Key::Special(ScanCode::RIGHT)) => {
                y += 10;
                game = true;
            }
            Some(Key::Special(ScanCode::UP)) => {
                x -= 10;
                game = true;
            }
            Some(Key::Special(ScanCode::DOWN)) => {
                x += 10;
                game = true;
            }

            // Example of handling a special key: exit the loop when the
            // escape key is pressed.
            Some(Key::Special(ScanCode::ESCAPE)) => {
                break;
            }
            _ => {}
        };
        // X and Y are exchanged?
        if game {
            let image = Image::new(&bmp, Point::new(100, 100) + Point::new(y, x));
            image.draw(&mut buffer).unwrap();
        }
        buffer.blit(&mut gop)?;
    }
    Ok(())
}

pub fn print_gop_modes(bs: &BootServices) -> Result {
    // Open graphics output protocol.
    //let gop_handle = bs.get_handle_for_protocol::<GraphicsOutput>()?;
    // open_protocol_exclusive will steal it from anyone else, meaning shell prints won't work anymore
    //let mut gop = bs.open_protocol_exclusive::<GraphicsOutput>(gop_handle)?;

    let gop_handles = if let Ok(gop_handles) =
        bs.locate_handle_buffer(SearchType::ByProtocol(&GraphicsOutput::GUID))
    {
        gop_handles
    } else {
        info!("GraphicsOutput protocol is missing");
        return Ok(());
    };

    // TODO: Why are there two? I think we can just use the first one?
    // They seem to have the same information
    //if let Some(handle) = gop_handles.iter().next() {
    for handle in gop_handles.iter() {
        if let Ok(dp) = unsafe {
            bs.open_protocol::<DevicePath>(
                OpenProtocolParams {
                    handle: *handle,
                    agent: bs.image_handle(),
                    controller: None,
                },
                OpenProtocolAttributes::GetProtocol,
            )
        } {
            print_dp(bs, &dp);
        } else {
            error!("No DP for this GOP handle");
        };

        let gop = unsafe {
            bs.open_protocol::<GraphicsOutput>(
                OpenProtocolParams {
                    handle: *handle,
                    agent: bs.image_handle(),
                    controller: None,
                },
                OpenProtocolAttributes::GetProtocol,
            )
            .expect("Failed to open GraphicsOutput handle")
        };

        let cur_mode_info = gop.current_mode_info();
        let (width, height) = cur_mode_info.resolution();
        info!("Current Resolution {}x{}", width, height);

        for (index, mode) in gop.modes().enumerate() {
            let info = mode.info();
            let (width, height) = info.resolution();
            info!(
                "{}{}Mode Resolution {}x{}, Stride: {}",
                index,
                if *info == cur_mode_info { "*" } else { " " },
                width,
                height,
                info.stride()
            );
            info!(
                "  Pixel format: {:?}, bitmask: {:?}",
                info.pixel_format(),
                info.pixel_bitmask()
            );
        }
    }
    Ok(())
}

fn print_dp(bs: &BootServices, dp: &ScopedProtocol<DevicePath>) {
    let device_path_to_text_handle = *bs
        .locate_handle_buffer(SearchType::ByProtocol(&DevicePathToText::GUID))
        .unwrap()
        .first()
        .expect("DevicePathToText is missing");

    let device_path_to_text = bs
        .open_protocol_exclusive::<DevicePathToText>(device_path_to_text_handle)
        .unwrap();

    let image_device_path_text = device_path_to_text
        .convert_device_path_to_text(bs, dp, DisplayOnly(true), AllowShortcuts(false))
        .expect("convert_device_path_to_text failed");

    info!("DevicePath: {}", &*image_device_path_text);
}
