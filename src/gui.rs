use alloc::vec;
use alloc::vec::Vec;
use embedded_graphics::image::{ImageRaw, SubImage};
use embedded_graphics::primitives::{PrimitiveStyle, PrimitiveStyleBuilder, Rectangle};
use framework_lib::chromium_ec::input_deck::{self, InputDeckState};
use framework_lib::chromium_ec::input_deck::{InputDeckStatus, InputModuleType};
#[cfg(not(feature = "qemu"))]
use framework_lib::chromium_ec::CrosEc;
#[allow(unused_imports)]
use log::{debug, error, info};
use uefi::proto::console::gop::{BltOp, BltPixel, BltRegion, GraphicsOutput};
use uefi::proto::console::text::{Input, Key, ScanCode};
use uefi::table::boot::BootServices;
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
#[cfg(feature = "bmp")]
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

fn show_top(
    show: bool,
    pos: usize,
    buffer: &mut Buffer,
    sprites: &[&SubImage<ImageRaw<Rgb888>>],
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
    touchpad_sprite: &SubImage<ImageRaw<Rgb888>>,
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
    #[cfg(not(feature = "qemu"))]
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

    cfg_if::cfg_if! {
        // Rects for QEMU (1024 x 768 x 24)
        if #[cfg(feature = "qemu")] {
            cfg_if::cfg_if! {
                if #[cfg(feature = "bmp")] {
                    //let background_image =
                    //    include_bytes!("../resources/framework-laptop-16-input-module-topdown-1024-768.bmp");
                    let background_image =
                        include_bytes!("../resources/framework-laptop-16-input-module-topdown-1024-768-noback.bmp");
                } else {
                    let jpeg =
                        include_bytes!("../resources/framework-laptop-16-input-module-topdown-1024-768-tiny.jpg");
                    let mut decoder = zune_jpeg::JpegDecoder::new(jpeg);
                    let background_image = decoder.decode().unwrap();
                }
            }
            let y_top = 360;
            let y_bottom = 545;
            let x_left = 255;
            let c_width = 310 - 255;
            let a_width = 464;
            let touchpad_height = 729 - 546;
            let text_pos = Point::new(250, 30);
        } else {
            // Rects for Lotus (2560 x 1600 x 24)
            cfg_if::cfg_if! {
                if #[cfg(feature = "bmp")] {
                    let background_image = include_bytes!("../resources/framework-laptop-16-input-module-topdown-2560x1600.bmp");
                } else {
                    let jpeg =
                        include_bytes!("../resources/framework-laptop-16-input-module-topdown-2560x1600-tiny.jpg");
                    let mut decoder = zune_jpeg::JpegDecoder::new(jpeg);
                    let background_image = decoder.decode().unwrap();
                }
            }
            let y_top = 736;
            let y_bottom = 1113;
            let x_left = 706;
            let c_width = 110;
            let a_width = 945;
            let touchpad_height = 372;
            let text_pos = Point::new(500, 30);
        }
    }

    // Have to export in GIMP as RGB888
    cfg_if::cfg_if! {
        if #[cfg(feature = "bmp")] {
            let background_bmp: Bmp<Rgb888> = Bmp::from_slice(background_image).unwrap();
            let image = Image::new(&background_bmp, Point::new(0, 0));
        } else {
            #[cfg(feature = "qemu")]
            let background_bmp = ImageRaw::<Rgb888>::new(&background_image, 1024);
            #[cfg(not(feature = "qemu"))]
            let background_bmp = ImageRaw::<Rgb888>::new(&background_image, 2560);
            let image = Image::new(&background_bmp, Point::new(0, 0));
        }
    }
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
        text_pos,
        MonoTextStyle::new(&FONT_9X15, Rgb888::BLACK),
    );

    text.draw(&mut buffer).unwrap();

    // Draw the buffer to the screen.
    buffer.blit(&mut gop)?;

    let input_handle = bs.get_handle_for_protocol::<Input>().unwrap();
    let mut input = bs.open_protocol_exclusive::<Input>(input_handle).unwrap();

    let zero_key = Char16::try_from('0').unwrap();
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
    let mut emulated_deck = InputDeckStatus {
        state: input_deck::InputDeckState::On,
        hubboard_present: true,
        touchpad_present: false,
        top_row: input_deck::TopRowPositions {
            pos0: input_deck::InputModuleType::Disconnected,
            pos1: input_deck::InputModuleType::Disconnected,
            pos2: input_deck::InputModuleType::Disconnected,
            pos3: input_deck::InputModuleType::Disconnected,
            pos4: input_deck::InputModuleType::Disconnected,
        },
    };
    loop {
        #[cfg(not(feature = "qemu"))]
        let input_deck = ec.get_input_deck_status();
        #[cfg(feature = "qemu")]
        let input_deck: core::result::Result<InputDeckStatus, ()> = core::result::Result::Err(());
        let input_deck = if let Ok(input_deck) = input_deck {
            //println!("Got it!");
            input_deck
        } else {
            //println!("Emulated deck data");
            emulated_deck.clone()
        };
        //debug!("InputDeck State: {:?}", input_deck.state);
        //debug!("Touchpad present: {:?}", input_deck.touchpad_present);
        //debug!("Hubboard present: {:?}", input_deck.hubboard_present);

        // Since there's no hubboard, there are no exposed pins that we should
        // avoid powering on.
        // Due to misconfiguration the user might be in this case, when running
        // a standalone mainboard. We should not block boot in this case.
        if !input_deck.hubboard_present {
            println!("No hub board present- continuing to boot");
            bs.stall(1_000_000);
            return Ok(());
        }
        if !matches!(
            input_deck.state,
            InputDeckState::Disconnected | InputDeckState::TurningOn | InputDeckState::On
        ) {
            println!("Input Deck State Overridden: {:?}. No detection of modules possible - continuing to boot ", input_deck.state);
            bs.stall(1_000_000);
            return Ok(());
        }

        // Reset to no modules, otherwise we can't reset what's shown
        show_top(false, 0, &mut buffer, &a1_sprites, &a1_rects);
        show_top(false, 2, &mut buffer, &c1_sprites, &c1_rects);
        show_top(false, 3, &mut buffer, &c1_sprites, &c1_rects);

        show_touchpad(
            input_deck.touchpad_present,
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

        match input.read_key().unwrap() {
            Some(Key::Printable(key)) if key == zero_key => {
                emulated_deck = InputDeckStatus {
                    state: input_deck::InputDeckState::On,
                    hubboard_present: true,
                    touchpad_present: false,
                    top_row: input_deck::TopRowPositions {
                        pos0: input_deck::InputModuleType::Disconnected,
                        pos1: input_deck::InputModuleType::Disconnected,
                        pos2: input_deck::InputModuleType::Disconnected,
                        pos3: input_deck::InputModuleType::Disconnected,
                        pos4: input_deck::InputModuleType::Disconnected,
                    },
                };
            }
            Some(Key::Printable(key)) if key == one_key => {
                emulated_deck = InputDeckStatus {
                    state: input_deck::InputDeckState::On,
                    hubboard_present: true,
                    touchpad_present: false,
                    top_row: input_deck::TopRowPositions {
                        pos0: input_deck::InputModuleType::GenericC,
                        pos1: input_deck::InputModuleType::Disconnected,
                        pos2: input_deck::InputModuleType::Disconnected,
                        pos3: input_deck::InputModuleType::Disconnected,
                        pos4: input_deck::InputModuleType::Disconnected,
                    },
                };
            }
            Some(Key::Printable(key)) if key == two_key => {
                emulated_deck = InputDeckStatus {
                    state: input_deck::InputDeckState::On,
                    hubboard_present: true,
                    touchpad_present: false,
                    top_row: input_deck::TopRowPositions {
                        pos0: input_deck::InputModuleType::GenericC,
                        pos1: input_deck::InputModuleType::GenericC,
                        pos2: input_deck::InputModuleType::Disconnected,
                        pos3: input_deck::InputModuleType::Disconnected,
                        pos4: input_deck::InputModuleType::Disconnected,
                    },
                };
            }
            Some(Key::Printable(key)) if key == three_key => {
                emulated_deck = InputDeckStatus {
                    state: input_deck::InputDeckState::On,
                    hubboard_present: true,
                    touchpad_present: false,
                    top_row: input_deck::TopRowPositions {
                        pos0: input_deck::InputModuleType::GenericC,
                        pos1: input_deck::InputModuleType::GenericC,
                        pos2: input_deck::InputModuleType::KeyboardA,
                        pos3: input_deck::InputModuleType::Disconnected,
                        pos4: input_deck::InputModuleType::Disconnected,
                    },
                };
            }
            Some(Key::Printable(key)) if key == four_key => {
                emulated_deck = InputDeckStatus {
                    state: input_deck::InputDeckState::On,
                    hubboard_present: true,
                    touchpad_present: true,
                    top_row: input_deck::TopRowPositions {
                        pos0: input_deck::InputModuleType::GenericC,
                        pos1: input_deck::InputModuleType::GenericC,
                        pos2: input_deck::InputModuleType::KeyboardA,
                        pos3: input_deck::InputModuleType::Disconnected,
                        pos4: input_deck::InputModuleType::Disconnected,
                    },
                };
            }

            // Example of handling a special key: exit the loop when the
            // escape key is pressed.
            Some(Key::Special(ScanCode::ESCAPE)) => {
                break;
            }
            _ => {}
        };
    }
    Ok(())
}
