use core::ffi::c_void;
use core::ptr::NonNull;

use alloc::format;
use embedded_graphics::image::ImageRaw;
use embedded_graphics::primitives::{
    PrimitiveStyle, PrimitiveStyleBuilder, Rectangle, RoundedRectangle,
};
use framework_lib::chromium_ec::input_deck::{self, InputDeckState};
use framework_lib::chromium_ec::input_deck::{InputDeckStatus, InputModuleType};
use framework_lib::chromium_ec::{CrosEc, EcRequestRaw};
#[allow(unused_imports)]
use log::{debug, error, info};
use uefi::proto::console::gop::GraphicsOutput;
use uefi::proto::console::text::{Input, Key, ScanCode};
use uefi::table::boot::{BootServices, EventType, Tpl};
use uefi::table::boot::{OpenProtocolAttributes, OpenProtocolParams, SearchType};
use uefi::Result;
#[allow(unused_imports)]
use uefi::{prelude::*, Char16};
use uefi::{Event, Identify};
#[allow(unused_imports)]
use uefi_services::{print, println};

use embedded_graphics::{
    image::Image,
    mono_font::{ascii, MonoTextStyle},
    pixelcolor::Rgb888,
    prelude::*,
    text::Text,
};

use crate::buffer::Buffer;
use crate::built_info;
use crate::EcRequestUefiAppBtnStatus;
use crate::EcRequestUefiAppMode;

const BG_COLOR: Rgb888 = Rgb888::new(0x28, 0x25, 0x24);
const RED_WARN: Rgb888 = Rgb888::new(0xff, 0xe1, 0xe0);
const RED_BORDER: Rgb888 = Rgb888::new(0xca, 0x1c, 0x16);
const GREEN_OK: Rgb888 = Rgb888::new(0xf5, 0xff, 0xfb);
const GREEN_BORDER: Rgb888 = Rgb888::new(0x00, 0x8a, 0x4e);

//const BG_COLOR_BLUE: Rgb888 = Rgb888::new(0xcb, 0xe6, 0xe6);
const COLORS: [(Rgb888, Rgb888, Rgb888); 2] = [
    (BG_COLOR, RED_WARN, RED_BORDER),
    (BG_COLOR, GREEN_OK, GREEN_BORDER),
];
const TEXT_OFFSET: i32 = 80;
const TEXT_PAD: u32 = 30;
const TEXT_YPAD: u32 = 45;
const TEXT_BORDER: u32 = 2;
const TEXT_LEFTPAD: i32 = (TEXT_PAD + TEXT_BORDER) as i32;

fn show_status(
    show: bool,
    buffer: &mut Buffer,
    text_pos: Point,
    tu_td: (&ImageRaw<Rgb888>, &ImageRaw<Rgb888>),
    font: &embedded_graphics::mono_font::MonoFont,
    text: &str,
    index: i32,
) {
    let (tu, td) = tu_td;
    let (fg_color, bg_color, border_color) = COLORS[show as usize];
    let text = Text::new(
        text,
        text_pos + Point::new(TEXT_LEFTPAD, TEXT_OFFSET * index),
        MonoTextStyle::new(font, fg_color),
    );

    let rect = text.bounding_box();
    let rect = rect.resized(
        rect.size + Size::new(TEXT_PAD * 2, TEXT_YPAD),
        embedded_graphics::geometry::AnchorPoint::Center,
    );
    let rect =
        RoundedRectangle::with_equal_corners(rect, Size::new(rect.size.height, rect.size.height));
    let blank_style: PrimitiveStyle<Rgb888> = PrimitiveStyleBuilder::new()
        .fill_color(bg_color)
        .stroke_width(TEXT_BORDER)
        .stroke_color(border_color)
        .build();
    rect.into_styled(blank_style).draw(buffer).unwrap();

    text.draw(buffer).unwrap();

    Image::new(
        if show { tu } else { td },
        text_pos + Point::new(TEXT_BORDER as i32 + 20, TEXT_OFFSET * index - 20),
    )
    .draw(buffer)
    .unwrap();
}

fn show_touchpad(
    show: bool,
    buffer: &mut Buffer,
    trackpad: &Image<ImageRaw<Rgb888>>,
    csize_bmp: &ImageRaw<Rgb888>,
    c_left: Point,
    c_right: Point,
) {
    if show {
        Image::new(csize_bmp, c_left).draw(buffer).unwrap();
        trackpad.draw(buffer).unwrap();
        Image::new(csize_bmp, c_right).draw(buffer).unwrap();
    }
}

fn get_input_deck(emulated_deck: &InputDeckStatus, _ec: &Option<CrosEc>) -> InputDeckStatus {
    #[cfg(not(feature = "qemu"))]
    let input_deck = _ec.as_ref().unwrap().get_input_deck_status();
    #[cfg(feature = "qemu")]
    let input_deck: core::result::Result<InputDeckStatus, ()> = core::result::Result::Err(());
    if let Ok(input_deck) = input_deck {
        //println!("Got it!");
        input_deck
    } else {
        //println!("Emulated deck data");
        emulated_deck.clone()
    }
}

pub fn draw_background(bs: &BootServices, _st: &SystemTable<Boot>, ec: &Option<CrosEc>) -> Result {
    let mut emulated_deck = InputDeckStatus {
        state: input_deck::InputDeckState::Disconnected,
        hubboard_present: false,
        touchpad_present: false,
        top_row: input_deck::TopRowPositions {
            pos0: input_deck::InputModuleType::Disconnected,
            pos1: input_deck::InputModuleType::Disconnected,
            pos2: input_deck::InputModuleType::Disconnected,
            pos3: input_deck::InputModuleType::Disconnected,
            pos4: input_deck::InputModuleType::Disconnected,
        },
    };
    let input_deck = get_input_deck(&emulated_deck, &ec);
    let mut pressed: u8 = 255;
    let mut ctr: u64 = 0;

    // If we start out as fully populated we want to boot as fast as possible
    // and skip the GUI
    if input_deck.fully_populated() {
        info!("Started out as fully populated, booting immediately");
        return Ok(());
    }

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
    let size = Point::new(width as i32, height as i32);
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

    // Rects for Lotus (2560 x 1600 x 24)
    let png = include_bytes!("../resources/baseboard.png");
    let mut decoder = zune_png::PngDecoder::new(png);
    let zune_core::result::DecodingResult::U8(background_image) = decoder.decode().unwrap() else { panic!("Failed to decode PNG") };

    let png = include_bytes!("../resources/instructions.png");
    let mut decoder = zune_png::PngDecoder::new(png);
    let zune_core::result::DecodingResult::U8(instructions_image) = decoder.decode().unwrap() else { panic!("Failed to decode PNG") };

    let png = include_bytes!("../resources/trackpad.png");
    let mut decoder = zune_png::PngDecoder::new(png);
    let zune_core::result::DecodingResult::U8(trackpad_image) = decoder.decode().unwrap() else { panic!("Failed to decode PNG") };

    let png = include_bytes!("../resources/csize.png");
    let mut decoder = zune_png::PngDecoder::new(png);
    let zune_core::result::DecodingResult::U8(csize_image) = decoder.decode().unwrap() else { panic!("Failed to decode PNG") };

    let png = include_bytes!("../resources/keyboard.png");
    let mut decoder = zune_png::PngDecoder::new(png);
    let zune_core::result::DecodingResult::U8(asize_image) = decoder.decode().unwrap() else { panic!("Failed to decode PNG") };

    let png = include_bytes!("../resources/numpad.png");
    let mut decoder = zune_png::PngDecoder::new(png);
    let zune_core::result::DecodingResult::U8(numpad_image) = decoder.decode().unwrap() else { panic!("Failed to decode PNG") };

    let png = include_bytes!("../resources/bsize.png");
    let mut decoder = zune_png::PngDecoder::new(png);
    let zune_core::result::DecodingResult::U8(bsize_image) = decoder.decode().unwrap() else { panic!("Failed to decode PNG") };

    let png = include_bytes!("../resources/thumbsup.png");
    let mut decoder = zune_png::PngDecoder::new(png);
    let zune_core::result::DecodingResult::U8(thumbsup_image) = decoder.decode().unwrap() else { panic!("Failed to decode PNG") };
    let png = include_bytes!("../resources/thumbsdown.png");
    let mut decoder = zune_png::PngDecoder::new(png);
    let zune_core::result::DecodingResult::U8(thumbsdown_image) = decoder.decode().unwrap() else { panic!("Failed to decode PNG") };

    let y_top = 756;
    let x_left = 1210;
    let a_width = 806;
    let b_width = 196;
    let c_width = 100;
    let touchpad_height = 324;
    let y_bottom = y_top + touchpad_height;
    let screen_width = 2560;
    let screen_height = 1600;
    let text_pos = Point::new(133, 280);
    let text2_pos = Point::new(133, 1360);
    let help1_pos = Point::new(205, 610);
    let help2_pos = Point::new(1090, 1390);
    let help3_pos = Point::new(1320, 610);
    let instr_font = ascii::FONT_9X18_BOLD; //_BOLD;
    let big_font = ascii::FONT_10X20;

    let left_c1_rect: Rectangle = Rectangle::with_corners(
        Point::new(x_left, y_top),
        Point::new(x_left + c_width, y_bottom),
    );
    let left2_c1_rect: Rectangle = Rectangle::with_corners(
        Point::new(x_left + c_width, y_top),
        Point::new(x_left + c_width * 2, y_bottom),
    );
    let left_a1_rect: Rectangle = Rectangle::with_corners(
        Point::new(x_left, y_top),
        Point::new(x_left + a_width, y_bottom),
    );
    let middle_a1_rect: Rectangle = Rectangle::with_corners(
        Point::new(x_left + c_width, y_top),
        Point::new(x_left + c_width + a_width, y_bottom),
    );
    let right_a1_rect: Rectangle = Rectangle::with_corners(
        Point::new(x_left + 2 * c_width, y_top),
        Point::new(x_left + 2 * c_width + a_width, y_bottom),
    );
    let right0_c1_rect: Rectangle = Rectangle::with_corners(
        Point::new(x_left + a_width, y_top),
        Point::new(x_left + a_width + c_width, y_bottom),
    );
    let right_c1_rect: Rectangle = Rectangle::with_corners(
        Point::new(x_left + a_width + c_width, y_top),
        Point::new(x_left + a_width + 2 * c_width, y_bottom),
    );
    let touchpad_rect: Rectangle = Rectangle::with_corners(
        Point::new(x_left + c_width, y_bottom),
        Point::new(x_left + c_width + a_width, y_bottom + touchpad_height),
    );

    // Have to export in GIMP as RGB888
    let background_bmp = ImageRaw::<Rgb888>::new(&background_image, screen_width);
    let image = Image::new(&background_bmp, Point::new(0, 0));

    let instructions_bmp = ImageRaw::<Rgb888>::new(&instructions_image, screen_width);
    let instructions_image = Image::new(&instructions_bmp, Point::new(0, 0));

    let trackpad_bmp = ImageRaw::<Rgb888>::new(&trackpad_image, a_width as u32);
    let trackpad_image = Image::new(&trackpad_bmp, touchpad_rect.top_left);

    let csize_bmp = ImageRaw::<Rgb888>::new(&csize_image, c_width as u32);
    let asize_bmp = ImageRaw::<Rgb888>::new(&asize_image, a_width as u32);
    let numpad_bmp = ImageRaw::<Rgb888>::new(&numpad_image, b_width as u32);
    let bsize_bmp = ImageRaw::<Rgb888>::new(&bsize_image, b_width as u32);

    let thumbsup_bmp = ImageRaw::<Rgb888>::new(&thumbsup_image, 38);
    let thumbsdown_bmp = ImageRaw::<Rgb888>::new(&thumbsdown_image, 38);

    draw_main(
        &mut buffer,
        &image,
        text_pos,
        text2_pos,
        size,
        &instr_font,
        &big_font,
        pressed,
        ctr,
    );
    let mut show_instructions = false;

    // Draw the buffer to the screen.
    buffer.blit(&mut gop)?;

    let input_handle = bs.get_handle_for_protocol::<Input>().unwrap();
    let mut input = bs.open_protocol_exclusive::<Input>(input_handle).unwrap();

    let zero_key = Char16::try_from('0').unwrap();
    let one_key = Char16::try_from('1').unwrap();
    let two_key = Char16::try_from('2').unwrap();
    let three_key = Char16::try_from('3').unwrap();
    let four_key = Char16::try_from('4').unwrap();
    let five_key = Char16::try_from('5').unwrap();
    let q_key = Char16::try_from('?').unwrap();
    let mut need_redraw = true;
    let mut prev_input_deck = input_deck;
    let mut prev_pressed = 100;
    let mut long_press = 0;
    loop {
        if let Some(ref ec) = ec {
            let btn_status = EcRequestUefiAppBtnStatus {}.send_command(&ec).unwrap();
            pressed = if btn_status.status == 0 { 1 } else { 0 };
            //if btn_status.flags != 0 {
            //    debug!("Power button pressed");
            //    bs.stall(1_000_000);
            //}
        }
        // Short press will exit
        if prev_pressed == 1 && pressed == 0 {
            return Ok(());
        }
        let input_deck = get_input_deck(&emulated_deck, &ec);
        need_redraw = input_deck != prev_input_deck || pressed != prev_pressed;

        if pressed == 1 && prev_pressed == 1 && !need_redraw {
            if long_press > 4 {
                if let Some(ref ec) = ec {
                    EcRequestUefiAppMode { enable: 0 }
                        .send_command(&ec)
                        .unwrap();
                }
                //TODO: REmove this
                println!("Shutting down;");
                bs.stall(1_000_000);
                // Shut down the system
                let rt = unsafe { _st.runtime_services() };
                rt.reset(
                    uefi::table::runtime::ResetType::Shutdown,
                    Status::SUCCESS,
                    None,
                );
            }
            // Wait a few seconds and if it's still pressed next time around, it was a long press
            bs.stall(1_000_000);
            long_press += 1;
        } else {
            long_press = 0;
        }
        prev_pressed = pressed;
        prev_input_deck = input_deck.clone();

        //debug!("InputDeck State: {:?}", input_deck.state);
        //debug!("Touchpad present: {:?}", input_deck.touchpad_present);
        //debug!("Hubboard present: {:?}", input_deck.hubboard_present);

        if !matches!(
            input_deck.state,
            InputDeckState::Disconnected | InputDeckState::TurningOn | InputDeckState::On
        ) {
            println!("Input Deck State Overridden: {:?}. No detection of modules possible - continuing to boot ", input_deck.state);
            bs.stall(1_000_000);
            return Ok(());
        }

        // Instructions don't need updates, they're only drawn once when the button is pressed
        if !show_instructions && need_redraw {
            ctr += 1;
            // Reset background to no modules, otherwise we can't reset what's
            // shown when modules are removed or move around.
            draw_main(
                &mut buffer,
                &image,
                text_pos,
                text2_pos,
                size,
                &instr_font,
                &big_font,
                pressed,
                ctr,
            );

            show_status(
                input_deck.hubboard_present,
                &mut buffer,
                text_pos,
                (&thumbsup_bmp, &thumbsdown_bmp),
                &big_font,
                "      Hubboard",
                2,
            );

            let top_row_present = input_deck
                .top_row_to_array()
                .iter()
                .map(InputModuleType::size)
                .sum::<usize>()
                == input_deck::INPUT_DECK_SLOTS;
            show_status(
                top_row_present,
                &mut buffer,
                text_pos,
                (&thumbsup_bmp, &thumbsdown_bmp),
                &big_font,
                "      Top Row",
                3,
            );

            show_status(
                input_deck.touchpad_present,
                &mut buffer,
                text_pos,
                (&thumbsup_bmp, &thumbsdown_bmp),
                &big_font,
                "      Touchpad",
                4,
            );
            show_touchpad(
                input_deck.touchpad_present,
                &mut buffer,
                &trackpad_image,
                &csize_bmp,
                Point::new(x_left, y_top + touchpad_height),
                Point::new(x_left + c_width + a_width, y_top + touchpad_height),
            );
            show_status(
                false,
                &mut buffer,
                text_pos,
                (&thumbsup_bmp, &thumbsdown_bmp),
                &big_font,
                "      Fully Populated",
                5,
            );
            if input_deck.top_row.pos0 == InputModuleType::GenericC {
                Image::new(&csize_bmp, left_c1_rect.top_left)
                    .draw(&mut buffer)
                    .unwrap();
            }
            if input_deck.top_row.pos1 == InputModuleType::GenericC {
                Image::new(&csize_bmp, left2_c1_rect.top_left)
                    .draw(&mut buffer)
                    .unwrap();
            }
            if input_deck.top_row.pos3 == InputModuleType::GenericC {
                Image::new(&csize_bmp, right0_c1_rect.top_left)
                    .draw(&mut buffer)
                    .unwrap();
            }
            if input_deck.top_row.pos4 == InputModuleType::GenericC {
                Image::new(&csize_bmp, right_c1_rect.top_left)
                    .draw(&mut buffer)
                    .unwrap();
            }

            if input_deck.top_row.pos0 == InputModuleType::GenericA
                || input_deck.top_row.pos0 == InputModuleType::KeyboardA
            {
                Image::new(&asize_bmp, left_a1_rect.top_left)
                    .draw(&mut buffer)
                    .unwrap();
            }
            if input_deck.top_row.pos1 == InputModuleType::GenericA
                || input_deck.top_row.pos1 == InputModuleType::KeyboardA
            {
                Image::new(&asize_bmp, middle_a1_rect.top_left)
                    .draw(&mut buffer)
                    .unwrap();
            }
            if input_deck.top_row.pos2 == InputModuleType::GenericA
                || input_deck.top_row.pos2 == InputModuleType::KeyboardA
            {
                Image::new(&asize_bmp, right_a1_rect.top_left)
                    .draw(&mut buffer)
                    .unwrap();
            }

            if input_deck.top_row.pos0 == InputModuleType::GenericB {
                Image::new(&bsize_bmp, left_c1_rect.top_left)
                    .draw(&mut buffer)
                    .unwrap();
            }
            if input_deck.top_row.pos0 == InputModuleType::KeyboardB {
                Image::new(&numpad_bmp, left_c1_rect.top_left)
                    .draw(&mut buffer)
                    .unwrap();
            }
            if input_deck.top_row.pos3 == InputModuleType::GenericB {
                Image::new(&bsize_bmp, right0_c1_rect.top_left)
                    .draw(&mut buffer)
                    .unwrap();
            }
            if input_deck.top_row.pos3 == InputModuleType::KeyboardB {
                Image::new(&numpad_bmp, right0_c1_rect.top_left)
                    .draw(&mut buffer)
                    .unwrap();
            }
            buffer.blit(&mut gop)?;
        }

        // Wait a bit, clear the screen and exit
        if input_deck.fully_populated() {
            show_status(
                true,
                &mut buffer,
                text_pos,
                (&thumbsup_bmp, &thumbsdown_bmp),
                &big_font,
                "      Fully Populated",
                5,
            );
            buffer.blit(&mut gop)?;
            bs.stall(2_000_000);

            // Clear screen to black. Otherwise the next app is responsible for clearing.
            // If the app was launched from the UEFI shell, the user would have to manually clear
            let whole_screen: Rectangle = Rectangle::with_corners(
                Point::new(0, 0),
                Point::new(screen_width as i32, screen_height),
            );
            let blank_style = PrimitiveStyleBuilder::new()
                .fill_color(Rgb888::BLACK)
                .build();
            whole_screen
                .into_styled(blank_style)
                .draw(&mut buffer)
                .unwrap();
            buffer.blit(&mut gop)?;

            return Ok(());
        }

        // Put CPU to sleep until key press or 10ms timeout
        //sleep_or(bs, 10, unsafe {
        //    Some(input.wait_for_key_event().unsafe_clone())
        //});

        match input.read_key().unwrap() {
            Some(Key::Printable(key)) if key == zero_key => {
                emulated_deck = InputDeckStatus {
                    state: input_deck::InputDeckState::Disconnected,
                    hubboard_present: false,
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
                    state: input_deck::InputDeckState::Disconnected,
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
            Some(Key::Printable(key)) if key == two_key => {
                emulated_deck = InputDeckStatus {
                    state: input_deck::InputDeckState::Disconnected,
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
            Some(Key::Printable(key)) if key == three_key => {
                emulated_deck = InputDeckStatus {
                    state: input_deck::InputDeckState::Disconnected,
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
            Some(Key::Printable(key)) if key == four_key => {
                emulated_deck = InputDeckStatus {
                    state: input_deck::InputDeckState::Disconnected,
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
            Some(Key::Printable(key)) if key == five_key => {
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
            Some(Key::Printable(key)) if key == q_key => {
                show_instructions = !show_instructions;
                if show_instructions {
                    instructions_image.draw(&mut buffer).unwrap();

                    let text = Text::new(
                    "How to insert top row\n\nAlign to top, while keeping\nthe module at an angle,\nthen snap down to place.",
                        help1_pos,
                        MonoTextStyle::new(&instr_font, Rgb888::BLACK),
                    );
                    text.draw(&mut buffer).unwrap();

                    let text = Text::new(
                        "Press in to lock",
                        help2_pos,
                        MonoTextStyle::new(&instr_font, Rgb888::BLACK),
                    );
                    text.draw(&mut buffer).unwrap();

                    let text = Text::new(
                    "How to insert bottom row\n\nBegin at the bottom of the\nlaptop and slide up into place.",
                        help3_pos,
                        MonoTextStyle::new(&instr_font, Rgb888::BLACK),
                    );
                    text.draw(&mut buffer).unwrap();

                    // TODO: Info about power button is currently hidden until it's implemented
                    // "Press the power button or\nESC (on external keyboard)\nto skip the checks.\n\nPress ? to go back to overview.",
                    let text = Text::new(
                        "Press the ESC (on external keyboard)\nto skip the checks.\n\nPress ? to go back to overview.",
                        text2_pos,
                        MonoTextStyle::new(&instr_font, Rgb888::BLACK),
                    );
                    text.draw(&mut buffer).unwrap();

                    buffer.blit(&mut gop)?;
                } else {
                    draw_main(
                        &mut buffer,
                        &image,
                        text_pos,
                        text2_pos,
                        size,
                        &instr_font,
                        &big_font,
                        pressed,
                        ctr,
                    );
                }
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

fn draw_main(
    buffer: &mut Buffer,
    image: &Image<ImageRaw<Rgb888>>,
    text_pos: Point,
    text2_pos: Point,
    size: Point,
    instr_font: &embedded_graphics::mono_font::MonoFont,
    big_font: &embedded_graphics::mono_font::MonoFont,
    pressed: u8,
    ctr: u64,
) {
    image.draw(buffer).unwrap();
    // TODO: Use Roboto Mono font
    let text = Text::new(
        "Input Module Status",
        text_pos,
        MonoTextStyle::new(big_font, Rgb888::WHITE),
    );
    text.draw(buffer).unwrap();
    // TODO: Info about power button is currently hidden until it's implemented
    // "Press the power button or\nESC (on external keyboard)\nto skip the checks.\n\nPress ? for help.",
    let text = Text::new(
        "Press ESC (on external keyboard)\nto skip the checks.\n\nPress ? for help.",
        text2_pos,
        MonoTextStyle::new(instr_font, Rgb888::WHITE),
    );
    text.draw(buffer).unwrap();

    // Place version information in the lower right corner
    let ver_font = instr_font;
    let cs = Point::new(
        ver_font.character_size.width as i32,
        ver_font.character_size.height as i32,
    );
    let (height, ver_text) = if built_info::DEBUG {
        (
            4,
            format!(
                "Version: {}\nRef:     {}\nCommit:  {}\nDirty:   {}, BTN: {}, ctr: {}",
                built_info::PKG_VERSION,
                built_info::GIT_HEAD_REF.unwrap_or("?"),
                built_info::GIT_COMMIT_HASH.unwrap_or("?"),
                built_info::GIT_DIRTY.unwrap_or(false),
                pressed,
                ctr
            ),
        )
    } else {
        (1, format!("v{}", built_info::PKG_VERSION))
    };
    let ver_len = ver_text.lines().fold(0, |longest, cur| -> i32 {
        let cur_len = cur.len() as i32;
        if longest > cur_len {
            longest
        } else {
            cur_len
        }
    });
    let text = Text::new(
        &ver_text,
        size - Point::new(cs.x * ver_len + cs.x, cs.y * height),
        MonoTextStyle::new(ver_font, Rgb888::WHITE),
    );
    text.draw(buffer).unwrap();
}

unsafe extern "efiapi" fn idle_callback(_event: Event, _context: Option<NonNull<c_void>>) {}

fn sleep_or(bs: &BootServices, ms: u64, or_event: Option<Event>) {
    let oneshot_timer = unsafe {
        bs.create_event(EventType::TIMER, Tpl::CALLBACK, Some(idle_callback), None)
            .unwrap()
    };

    bs.set_timer(
        &oneshot_timer,
        uefi::table::boot::TimerTrigger::Relative(ms * 10_000),
    )
    .unwrap();

    let mut events = if let Some(event) = or_event {
        [oneshot_timer, event]
    } else {
        [unsafe { oneshot_timer.unsafe_clone() }, oneshot_timer]
    };
    let _which_event_fired = bs.wait_for_event(&mut events).discard_errdata().unwrap();
}
