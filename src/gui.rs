use alloc::vec;
use alloc::vec::Vec;
use embedded_graphics::image::SubImage;
use embedded_graphics::primitives::{PrimitiveStyle, PrimitiveStyleBuilder, Rectangle};
use framework_lib::chromium_ec::input_deck::{InputDeckStatus, InputModuleType};
use framework_lib::chromium_ec::{input_deck, CrosEc};
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
#[allow(unused_imports)]
use uefi_services::{print, println};

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

fn show_top(
    show: bool,
    pos: usize,
    buffer: &mut Buffer,
    sprites: &[&SubImage<Bmp<Rgb888>>],
    rects: &[Rectangle],
) {
    let sprite = sprites[pos];
    let rect = rects[pos];
    let blank_style: PrimitiveStyle<Rgb888> = PrimitiveStyleBuilder::new()
        .fill_color(Rgb888::CSS_GRAY)
        .build();
    if show {
        Image::new(sprite, rect.top_left).draw(buffer).unwrap();
    } else {
        rect.into_styled(blank_style).draw(buffer).unwrap();
    }
}

fn show_touchpad(
    show: bool,
    buffer: &mut Buffer,
    touchpad_sprite: &SubImage<Bmp<Rgb888>>,
    touchpad_rect: &Rectangle,
) {
    let blank_style: PrimitiveStyle<Rgb888> = PrimitiveStyleBuilder::new()
        .fill_color(Rgb888::CSS_GRAY)
        .build();
    if show {
        Image::new(touchpad_sprite, touchpad_rect.top_left)
            .draw(buffer)
            .unwrap();
    } else {
        touchpad_rect.into_styled(blank_style).draw(buffer).unwrap();
    }
}

pub fn draw_background(bs: &BootServices, _st: &SystemTable<Boot>) -> Result {
    let ec = CrosEc::new();
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

    // Rects for QEMU (1024 x 768 x 24)
    let background_image =
        include_bytes!("/home/zoid/Downloads/framework-laptop-16-input-module-topdown.bmp");
    let y_top = 360;
    let y_bottom = 545;
    let x_left = 255;
    let c_width = 310 - 255;
    let a_width = 464;
    let touchpad_height = 729 - 546;

    // Rects for Lotus (2560 x 1600 x 24)
    //let background_image = include_bytes!("/home/zoid/Downloads/framework-laptop-16-input-module-topdown-2560x1600.bmp");
    //let y_top = 736;
    //let y_bottom = 1113;
    //let x_left = 706;
    //let c_width = 110;
    //let a_width = 945;
    //let touchpad_height = 372;

    let bmp: Bmp<Rgb888> = Bmp::from_slice(include_bytes!(
        "../../led_matrix_fw/fl16-inputmodules/assets/logo.bmp"
    ))
    .unwrap();
    let image = Image::new(&bmp, Point::new(100, 100));
    image.draw(&mut buffer).unwrap();

    // Have to export in GIMP as RGB888
    let background_bmp: Bmp<Rgb888> = Bmp::from_slice(background_image).unwrap();
    let image = Image::new(&background_bmp, Point::new(0, 0));
    image.draw(&mut buffer).unwrap();

    let blank_style: PrimitiveStyle<Rgb888> = PrimitiveStyleBuilder::new()
        .fill_color(Rgb888::CSS_GRAY)
        .build();

    let left_c1_rect: Rectangle = Rectangle::with_corners(
        Point::new(x_left, y_top),
        Point::new(x_left + c_width, y_bottom),
    );
    let left2_c1_rect: Rectangle = Rectangle::with_corners(
        Point::new(x_left + c_width + 1, y_top),
        Point::new(x_left + c_width * 2 + 1, y_bottom),
    );
    let left_a1_rect: Rectangle = Rectangle::with_corners(
        Point::new(x_left, y_top),
        Point::new(x_left + a_width, y_bottom),
    );
    let middle_a1_rect: Rectangle = Rectangle::with_corners(
        Point::new(x_left + c_width + 1, y_top),
        Point::new(x_left + c_width + 1 + a_width, y_bottom),
    );
    let right_a1_rect: Rectangle = Rectangle::with_corners(
        Point::new(x_left + 2 * c_width + 2, y_top),
        Point::new(x_left + 2 * c_width + 2 + a_width, y_bottom),
    );
    let right0_c1_rect: Rectangle = Rectangle::with_corners(
        Point::new(x_left + a_width + 1, y_top),
        Point::new(x_left + a_width + 1 + c_width, y_bottom),
    );
    let right_c1_rect: Rectangle = Rectangle::with_corners(
        Point::new(x_left + a_width + 1 + c_width + 1, y_top),
        Point::new(x_left + a_width + 1 + 2 * c_width + 1, y_bottom),
    );
    let touchpad_rect: Rectangle = Rectangle::with_corners(
        Point::new(x_left + c_width + 1, y_bottom + 1),
        Point::new(
            x_left + c_width + 1 + a_width,
            y_bottom + 1 + touchpad_height,
        ),
    );

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

    touchpad_rect
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
    let four_key = Char16::try_from('4').unwrap();
    let left_c1_sprite = background_bmp.sub_image(&left_c1_rect);
    let middle_a1_sprite = background_bmp.sub_image(&middle_a1_rect);
    let right_c1_sprite = background_bmp.sub_image(&right_c1_rect);
    let touchpad_sprite = background_bmp.sub_image(&touchpad_rect);
    let c1_sprites = [
        &left_c1_sprite,
        &left_c1_sprite,
        &right_c1_sprite,
        &right_c1_sprite,
    ];
    let c1_rects = [left_c1_rect, left2_c1_rect, right0_c1_rect, right_c1_rect];
    let a1_sprites = [&middle_a1_sprite, &middle_a1_sprite, &middle_a1_sprite];
    let a1_rects = [left_a1_rect, middle_a1_rect, right_a1_rect];
    let mut left_c1_state = false;
    let mut middle_a1_state = false;
    let mut right_c1_state = false;
    let mut touchpad_state = false;
    loop {
        let input_deck = ec.get_input_deck_status();
        let input_deck = if let Ok(input_deck) = input_deck {
            input_deck
        } else {
            //println!("Emulated deck data");
            InputDeckStatus {
                state: input_deck::InputDeckState::On,
                touchpad_present: false,
                top_row: input_deck::TopRowPositions {
                    pos0: input_deck::InputModuleType::GenericC,
                    pos1: input_deck::InputModuleType::GenericA,
                    pos2: input_deck::InputModuleType::Disconnected,
                    pos3: input_deck::InputModuleType::Disconnected,
                    pos4: input_deck::InputModuleType::Disconnected,
                },
            }
        };
        //println!("InputDeckStatus: {:?}", input_deck);
        //println!("Touchpad present: {:?}", input_deck.touchpad_present);
        //show_input_deck(&mut buffer, &mut gop, &input_deck).unwrap();

        // Reset to no modules, otherwise we can't reset what's shown
        show_top(false, 0, &mut buffer, &a1_sprites, &a1_rects);
        show_top(false, 2, &mut buffer, &c1_sprites, &c1_rects);
        show_top(false, 3, &mut buffer, &c1_sprites, &c1_rects);

        touchpad_state = input_deck.touchpad_present;
        show_touchpad(
            touchpad_state,
            &mut buffer,
            &touchpad_sprite,
            &touchpad_rect,
        );
        if input_deck.top_row.pos0 == InputModuleType::GenericC {
            show_top(true, 0, &mut buffer, &c1_sprites, &c1_rects);
        }
        if input_deck.top_row.pos1 == InputModuleType::GenericC {
            show_top(true, 1, &mut buffer, &c1_sprites, &c1_rects);
        }
        if input_deck.top_row.pos3 == InputModuleType::GenericC {
            show_top(true, 2, &mut buffer, &c1_sprites, &c1_rects);
        }
        if input_deck.top_row.pos4 == InputModuleType::GenericC {
            show_top(true, 3, &mut buffer, &c1_sprites, &c1_rects);
        }

        if input_deck.top_row.pos0 == InputModuleType::GenericA
            || input_deck.top_row.pos0 == InputModuleType::KeyboardA
        {
            show_top(true, 0, &mut buffer, &a1_sprites, &a1_rects);
        }
        if input_deck.top_row.pos1 == InputModuleType::GenericA
            || input_deck.top_row.pos1 == InputModuleType::KeyboardA
        {
            show_top(true, 1, &mut buffer, &a1_sprites, &a1_rects);
        }
        if input_deck.top_row.pos2 == InputModuleType::GenericA
            || input_deck.top_row.pos2 == InputModuleType::KeyboardA
        {
            show_top(true, 2, &mut buffer, &a1_sprites, &a1_rects);
        }

        if input_deck.top_row.pos0 == InputModuleType::GenericB
            || input_deck.top_row.pos0 == InputModuleType::KeyboardB
        {
            // Just show two A1 for now
            // TODO: Make it clear that it's a single module
            show_top(true, 0, &mut buffer, &c1_sprites, &c1_rects);
            show_top(true, 1, &mut buffer, &c1_sprites, &c1_rects);
        }
        if input_deck.top_row.pos3 == InputModuleType::GenericB
            || input_deck.top_row.pos3 == InputModuleType::KeyboardB
        {
            // Just show two A1 for now
            // TODO: Make it clear that it's a single module
            show_top(true, 2, &mut buffer, &c1_sprites, &c1_rects);
            show_top(true, 3, &mut buffer, &c1_sprites, &c1_rects);
        }

        buffer.blit(&mut gop)?;

        if input_deck.fully_populated() {
            println!("Fully populated! Continuing boot");
            bs.stall(1_000_000);
            return Ok(());
        }

        // Pause until a keyboard event occurs.
        //let mut events = unsafe { [input.wait_for_key_event().unsafe_clone()] };
        //bs.wait_for_event(&mut events).discard_errdata().unwrap();

        // Update once a second
        bs.stall(1_000_000);

        //let u_key = Char16::try_from('u').unwrap();
        match input.read_key().unwrap() {
            // Example of handling a printable key: print a message when
            // the 'u' key is pressed.
            //Some(Key::Printable(key)) if key == u_key => {
            //    info!("the 'u' key was pressed");
            //}
            //Some(Key::Printable(key)) if key == one_key => {
            //    left_c1_state = !left_c1_state;
            //    show_top(left_c1_state, 0, &mut buffer, &c1_sprites, &c1_rects);
            //}
            //Some(Key::Printable(key)) if key == two_key => {
            //    middle_a1_state = !middle_a1_state;
            //    if middle_a1_state {
            //        Image::new(&middle_a1_sprite, middle_a1_rect.top_left)
            //            .draw(&mut buffer)
            //            .unwrap();
            //    } else {
            //        middle_a1_rect
            //            .into_styled(blank_style)
            //            .draw(&mut buffer)
            //            .unwrap();
            //    }
            //}
            //Some(Key::Printable(key)) if key == three_key => {
            //    right_c1_state = !right_c1_state;
            //    // TODO: Update number
            //    show_top(right_c1_state, 1, &mut buffer, &c1_sprites, &c1_rects);
            //}
            //Some(Key::Printable(key)) if key == four_key => {
            //    touchpad_state = !touchpad_state;
            //    show_touchpad(touchpad_state, &mut buffer, &touchpad_sprite, &touchpad_rect);
            //}
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

fn show_input_deck(
    buffer: &mut Buffer,
    gop: &mut ScopedProtocol<GraphicsOutput>,
    input_deck: &InputDeckStatus,
) -> Result {
    //buffer.blit(gop)?;
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
