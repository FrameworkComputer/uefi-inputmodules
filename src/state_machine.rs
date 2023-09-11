use alloc::format;
use alloc::vec::Vec;
use embedded_graphics::geometry::{AnchorPoint, Dimensions, Size};
use embedded_graphics::image::{Image, ImageRaw};
use embedded_graphics::mono_font::{ascii, MonoTextStyle};
use embedded_graphics::pixelcolor::Rgb888;
use embedded_graphics::prelude::{Point, Primitive, RgbColor};
use embedded_graphics::primitives::{
    PrimitiveStyle, PrimitiveStyleBuilder, Rectangle, RoundedRectangle,
};
use embedded_graphics::text::Text;
use embedded_graphics::Drawable;
use framework_lib::chromium_ec::commands::{
    EcRequestExpansionBayStatus, EcResponseExpansionBayStatus, ExpansionByStates,
};
use framework_lib::chromium_ec::input_deck::InputDeckStatus;
use framework_lib::chromium_ec::input_deck::{InputDeckState, InputModuleType, TopRowPositions};
use framework_lib::chromium_ec::{CrosEc, EcRequestRaw};
#[allow(unused_imports)]
use log::{debug, error, info, trace};
use uefi::prelude::BootServices;
use uefi::proto::console::gop::GraphicsOutput;
use uefi::table::boot::{OpenProtocolAttributes, OpenProtocolParams, ScopedProtocol, SearchType};
use uefi::table::{Boot, SystemTable};
use uefi::Identify;
use zune_core::result::DecodingResult;
use zune_png::PngDecoder;

use crate::buffer::Buffer;
use crate::built_info;

const WIDTH: usize = 2560;
const WIDTH_I32: i32 = 2560;
const HEIGHT: usize = 1600;
const HEIGHT_I32: i32 = 1600;

pub struct State {
    bitmaps: StateBitmaps,
    screen_size: Size,
    frame: StateFrame,
    prev_input_deck: Option<InputDeckStatus>,
}

#[derive(Default)]
struct StateBitmaps {
    qr_code_left: Option<Vec<u8>>,
    button_long_off: Option<Vec<u8>>,
    button_short_skip: Option<Vec<u8>>,
    laptop_latches_open: Option<Vec<u8>>,
    laptop_latches_closed: Option<Vec<u8>>,
    no_expansion_bay_instr: Option<Vec<u8>>,
    no_hubboard_instr: Option<Vec<u8>>,
    touchpad_no_modules_instr: Option<Vec<u8>>,
    inputmodules_missing_instr: Option<Vec<u8>>,
    no_touchpad_instr: Option<Vec<u8>>,
    latch_open_help_l: Option<Vec<u8>>,
    latch_open_help_r: Option<Vec<u8>>,
    latch_close_help_l: Option<Vec<u8>>,
    latch_close_help_r: Option<Vec<u8>>,
    touchpad: Option<Vec<u8>>,
    keyboard: Option<Vec<u8>>,
    b1_module: Option<Vec<u8>>,
    c1_module: Option<Vec<u8>>,
    hubboard_missing: Option<Vec<u8>>,
    expansion_bay_populated: Option<Vec<u8>>,
}

struct ImagePlusMeta<'a> {
    pub data: &'a [u8],
    pub width: u32,
    pub pos: Point,
}

impl<'a> ImagePlusMeta<'a> {
    pub fn draw(&self, buffer: &mut Buffer) {
        let image_raw = ImageRaw::<Rgb888>::new(self.data, self.width);
        let image = Image::new(&image_raw, self.pos);
        image.draw(buffer).unwrap();
    }

    // Draw at a different position
    pub fn draw_pos(&self, buffer: &mut Buffer, pos: Point) {
        let image_raw = ImageRaw::<Rgb888>::new(self.data, self.width);
        let image = Image::new(&image_raw, pos);
        image.draw(buffer).unwrap();
    }
}

impl StateBitmaps {
    pub fn qr_code_left(&mut self) -> ImagePlusMeta {
        if self.qr_code_left.is_none() {
            let png = include_bytes!("../resources/figma/qrcode-FRANNB.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else { panic!("Failed to decode PNG") };
            self.qr_code_left = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: &self.qr_code_left.as_ref().unwrap(),
            width: 132,
            pos: Point {
                x: 40,
                y: HEIGHT_I32 - 40 - 171,
            },
        }
    }

    pub fn button_long_off(&mut self) -> ImagePlusMeta {
        if self.button_long_off.is_none() {
            let png = include_bytes!("../resources/figma/button-long-off.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else { panic!("Failed to decode PNG") };
            self.button_long_off = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: &self.button_long_off.as_ref().unwrap(),
            width: 132,
            pos: Point {
                x: WIDTH_I32 - 40 - 132,
                y: HEIGHT_I32 - 40 - 171,
            },
        }
    }

    pub fn button_short_skip(&mut self) -> ImagePlusMeta {
        if self.button_short_skip.is_none() {
            let png = include_bytes!("../resources/figma/button-short-skip.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else { panic!("Failed to decode PNG") };
            self.button_short_skip = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: &self.button_short_skip.as_ref().unwrap(),
            width: 132,
            pos: Point {
                x: WIDTH_I32 - 40 - 2 * 132 - 20,
                y: HEIGHT_I32 - 40 - 171,
            },
        }
    }

    pub fn laptop_latches_closed(&mut self) -> ImagePlusMeta {
        if self.laptop_latches_closed.is_none() {
            let png = include_bytes!("../resources/figma/laptop-latch-closed.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else { panic!("Failed to decode PNG") };
            self.laptop_latches_closed = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: &self.laptop_latches_closed.as_ref().unwrap(),
            width: 1296,
            pos: Point { x: 180, y: 263 },
        }
    }

    pub fn laptop_latches_open(&mut self) -> ImagePlusMeta {
        if self.laptop_latches_open.is_none() {
            let png = include_bytes!("../resources/figma/laptop-latch-open.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else { panic!("Failed to decode PNG") };
            // TODO: Add more of this assertion
            assert_eq!(image_bytes.len() % 1296, 0);
            self.laptop_latches_open = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: &self.laptop_latches_open.as_ref().unwrap(),
            width: 1296,
            pos: Point { x: 180, y: 263 },
        }
    }

    pub fn no_expansion_bay_instr(&mut self) -> ImagePlusMeta {
        if self.no_expansion_bay_instr.is_none() {
            let png = include_bytes!("../resources/figma/no-expansion-bay-instructions.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else { panic!("Failed to decode PNG") };
            self.no_expansion_bay_instr = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: &self.no_expansion_bay_instr.as_ref().unwrap(),
            width: 761,
            pos: Point {
                x: WIDTH_I32 - 218 - 761,
                y: 264,
            },
        }
    }

    pub fn no_hubboard_instr(&mut self) -> ImagePlusMeta {
        if self.no_hubboard_instr.is_none() {
            let png = include_bytes!("../resources/figma/no-hubboard-instructions.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else { panic!("Failed to decode PNG") };
            self.no_hubboard_instr = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: &self.no_hubboard_instr.as_ref().unwrap(),
            width: 761,
            pos: Point {
                x: WIDTH_I32 - 218 - 761,
                y: 567,
            },
        }
    }

    pub fn touchpad_no_modules_instr(&mut self) -> ImagePlusMeta {
        if self.touchpad_no_modules_instr.is_none() {
            let png = include_bytes!("../resources/figma/touchpad-no-modules-instructions.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else { panic!("Failed to decode PNG") };
            self.touchpad_no_modules_instr = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: &self.touchpad_no_modules_instr.as_ref().unwrap(),
            width: 761,
            pos: Point {
                x: WIDTH_I32 - 218 - 761,
                y: 567,
            },
        }
    }

    pub fn inputmodules_missing_instr(&mut self) -> ImagePlusMeta {
        if self.inputmodules_missing_instr.is_none() {
            let png = include_bytes!("../resources/figma/inputmodules-missing-instructions.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else { panic!("Failed to decode PNG") };
            self.inputmodules_missing_instr = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: &self.inputmodules_missing_instr.as_ref().unwrap(),
            width: 761,
            pos: Point {
                x: WIDTH_I32 - 218 - 761,
                y: 567,
            },
        }
    }

    pub fn no_touchpad_instr(&mut self) -> ImagePlusMeta {
        if self.no_touchpad_instr.is_none() {
            let png = include_bytes!("../resources/figma/no-touchpad-instructions.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else { panic!("Failed to decode PNG") };
            self.no_touchpad_instr = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: &self.no_touchpad_instr.as_ref().unwrap(),
            width: 761,
            pos: Point {
                x: WIDTH_I32 - 218 - 761,
                y: 567,
            },
        }
    }

    pub fn latch_open_help_l(&mut self) -> ImagePlusMeta {
        if self.latch_open_help_l.is_none() {
            let png = include_bytes!("../resources/figma/latch-open-help-l.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else { panic!("Failed to decode PNG") };
            self.latch_open_help_l = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: &self.latch_open_help_l.as_ref().unwrap(),
            width: 181,
            pos: Point {
                x: 299 - 181,
                y: 1091,
            },
        }
    }

    pub fn latch_open_help_r(&mut self) -> ImagePlusMeta {
        if self.latch_open_help_r.is_none() {
            let png = include_bytes!("../resources/figma/latch-open-help-r.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else { panic!("Failed to decode PNG") };
            self.latch_open_help_r = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: &self.latch_open_help_r.as_ref().unwrap(),
            width: 181,
            pos: Point { x: 1364, y: 1091 },
        }
    }

    pub fn latch_close_help_l(&mut self) -> ImagePlusMeta {
        if self.latch_close_help_l.is_none() {
            let png = include_bytes!("../resources/figma/latch-close-help-l.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else { panic!("Failed to decode PNG") };
            self.latch_close_help_l = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: &self.latch_close_help_l.as_ref().unwrap(),
            width: 181,
            pos: Point {
                x: 300 - 181,
                y: 1091,
            },
        }
    }

    pub fn latch_close_help_r(&mut self) -> ImagePlusMeta {
        if self.latch_close_help_r.is_none() {
            let png = include_bytes!("../resources/figma/latch-close-help-r.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else { panic!("Failed to decode PNG") };
            self.latch_close_help_r = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: &self.latch_close_help_r.as_ref().unwrap(),
            width: 180,
            pos: Point { x: 1355, y: 1091 },
        }
    }

    pub fn touchpad(&mut self) -> ImagePlusMeta {
        if self.touchpad.is_none() {
            let png = include_bytes!("../resources/figma/touchpad.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else { panic!("Failed to decode PNG") };
            self.touchpad = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: &self.touchpad.as_ref().unwrap(),
            width: 997,
            pos: Point {
                x: 329,
                y: HEIGHT_I32 - 263 - 263,
            },
        }
    }

    pub fn keyboard(&mut self) -> ImagePlusMeta {
        if self.keyboard.is_none() {
            let png = include_bytes!("../resources/figma/keyboard.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else { panic!("Failed to decode PNG") };
            self.keyboard = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: &self.keyboard.as_ref().unwrap(),
            width: 804,
            // Middle position
            pos: Point {
                x: 329 + 97,
                y: HEIGHT_I32 - 263 - 263 - 319,
            },
        }
    }

    pub fn b1_module(&mut self) -> ImagePlusMeta {
        if self.b1_module.is_none() {
            let png = include_bytes!("../resources/figma/b1-module.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else { panic!("Failed to decode PNG") };
            self.b1_module = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: &self.b1_module.as_ref().unwrap(),
            width: 195,
            // Top left position
            pos: Point {
                x: 329,
                y: HEIGHT_I32 - 263 - 263 - 319,
            },
        }
    }

    pub fn c1_module(&mut self) -> ImagePlusMeta {
        if self.c1_module.is_none() {
            let png = include_bytes!("../resources/figma/c1-module.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else { panic!("Failed to decode PNG") };
            self.c1_module = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: &self.c1_module.as_ref().unwrap(),
            width: 98,
            // Top left position
            pos: Point {
                x: 329,
                y: HEIGHT_I32 - 263 - 263 - 319,
            },
        }
    }

    pub fn hubboard_missing(&mut self) -> ImagePlusMeta {
        if self.hubboard_missing.is_none() {
            let png = include_bytes!("../resources/figma/hubboard-missing.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else { panic!("Failed to decode PNG") };
            self.hubboard_missing = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: &self.hubboard_missing.as_ref().unwrap(),
            width: 87,
            pos: Point { x: 180, y: 263 } + Point { x: 978, y: 715 },
        }
    }

    pub fn expansion_bay_populated(&mut self) -> ImagePlusMeta {
        if self.expansion_bay_populated.is_none() {
            let png = include_bytes!("../resources/figma/expansion-bay-installed.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else { panic!("Failed to decode PNG") };
            self.expansion_bay_populated = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: &self.expansion_bay_populated.as_ref().unwrap(),
            width: 1007,
            pos: Point { x: 180, y: 263 } + Point { x: 144, y: 376 },
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum StateFrame {
    // If: No ExpansionBay Module
    // Short: Skip all, Long: Power Off
    // Extra Info: None, Maybe Expansion Bay ID
    NoExpansionBay,
    // If: No Hubboard
    // Short: Skip all, Long: Power Off
    // Extra Info: None
    NoHubboard,
    TouchpadButNoInputmodules,
    InputModulesMissing,
    NoTouchpad,
    Finished,
}

const BG_COLOR: Rgb888 = Rgb888::new(0x28, 0x25, 0x24);
const RED_WARN: Rgb888 = Rgb888::new(0xff, 0xe1, 0xe0);
const RED_BORDER: Rgb888 = Rgb888::new(0xca, 0x1c, 0x16);
const GREEN_OK: Rgb888 = Rgb888::new(0xf5, 0xff, 0xfb);
const GREEN_BORDER: Rgb888 = Rgb888::new(0x00, 0x8a, 0x4e);

enum TextColor {
    Warn,
    Ok,
    Info,
}
impl TextColor {
    pub fn color(&self) -> (Rgb888, Rgb888, Rgb888) {
        match self {
            TextColor::Warn => (BG_COLOR, RED_WARN, RED_BORDER),
            TextColor::Ok => (BG_COLOR, GREEN_OK, GREEN_BORDER),
            TextColor::Info => (BG_COLOR, Rgb888::WHITE, Rgb888::BLACK),
        }
    }
}
const TEXT_OFFSET: i32 = 80;
const TEXT_PAD: u32 = 30;
const TEXT_YPAD: u32 = 45;
const TEXT_BORDER: u32 = 2;
const TEXT_LEFTPAD: i32 = (TEXT_PAD + TEXT_BORDER) as i32;
fn draw_textbox(buffer: &mut Buffer, pos: Point, color: TextColor, text: &str) {
    let font = ascii::FONT_9X18_BOLD;
    //let font = ascii::FONT_10X20;
    let (fg_color, bg_color, border_color) = color.color();
    let text = Text::new(
        text,
        pos + Point::new(TEXT_LEFTPAD, TEXT_OFFSET),
        MonoTextStyle::new(&font, fg_color),
    );

    let rect = text.bounding_box();
    let rect = rect.resized(
        rect.size + Size::new(TEXT_PAD * 2, TEXT_YPAD),
        AnchorPoint::Center,
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
}

impl State {
    pub fn new() -> Self {
        Self {
            bitmaps: StateBitmaps::default(),
            screen_size: Size {
                width: WIDTH as u32,
                height: HEIGHT as u32,
            },
            //frame: StateFrame::NoExpansionBay,
            //frame: StateFrame::NoHubboard,
            //frame: StateFrame::InputModulesMissing,
            //frame: StateFrame::NoTouchpad,
            //frame: StateFrame::TouchpadButNoInputmodules,
            frame: StateFrame::Finished,
            prev_input_deck: None,
        }
    }
    // Very simple state machine
    // The next state does not depend on the current state
    pub fn next_state(
        &self,
        input_deck: &InputDeckStatus,
        expansion_bay: &EcResponseExpansionBayStatus,
    ) -> StateFrame {
        // Expected only as the first state
        // Otherwise won't usually happen because you're not supposed to hot-plug the module.
        // Can still happen as later state if the user opens the lid or hatch.
        if !expansion_bay.module_enabled() {
            return StateFrame::NoExpansionBay;
        }

        if !input_deck.hubboard_present {
            return StateFrame::NoHubboard;
        }

        match (
            input_deck.touchpad_present,
            input_deck.top_row_fully_populated(),
        ) {
            (false, false) => StateFrame::InputModulesMissing,
            (false, true) => StateFrame::NoTouchpad,
            (true, false) => StateFrame::TouchpadButNoInputmodules,
            (true, true) => StateFrame::Finished,
        }
    }
    pub fn draw_state(&mut self, buffer: &mut Buffer, input_deck: &InputDeckStatus) {
        // Draw solid background color #567676
        for y in 0..self.screen_size.height {
            for x in 0..self.screen_size.width {
                let pixel = buffer.pixel(x as usize, y as usize).unwrap();
                pixel.red = 0x56;
                pixel.green = 0x76;
                pixel.blue = 0x76;
            }
        }

        // Hints on lower left and right
        // Draw laptop with latches either closed or open
        match self.frame {
            StateFrame::Finished => {
                // Button indicator right
                self.bitmaps.button_short_skip().draw(buffer);

                // Draw laptop with latches closed
                self.bitmaps.laptop_latches_closed().draw(buffer);
            }
            _ => {
                // Step by step QR Code left
                self.bitmaps.qr_code_left().draw(buffer);

                // Button indicators right
                self.bitmaps.button_short_skip().draw(buffer);
                self.bitmaps.button_long_off().draw(buffer);

                // Draw laptop with latches open
                self.bitmaps.laptop_latches_open().draw(buffer);
                // Fix up the laptop
                if self.frame != StateFrame::NoExpansionBay {
                    self.bitmaps.expansion_bay_populated().draw(buffer)
                }
                if self.frame == StateFrame::NoHubboard {
                    self.bitmaps.hubboard_missing().draw(buffer)
                }
            }
        }

        // Instruction images
        match self.frame {
            StateFrame::NoExpansionBay => {
                self.bitmaps.no_expansion_bay_instr().draw(buffer);
            }
            StateFrame::NoHubboard => {
                self.bitmaps.no_hubboard_instr().draw(buffer);
            }
            StateFrame::TouchpadButNoInputmodules => {
                self.bitmaps.touchpad_no_modules_instr().draw(buffer);
            }
            StateFrame::InputModulesMissing => {
                self.bitmaps.inputmodules_missing_instr().draw(buffer);
            }
            StateFrame::NoTouchpad => {
                self.bitmaps.no_touchpad_instr().draw(buffer);
            }
            StateFrame::Finished => {}
        }

        // Latch help
        match self.frame {
            StateFrame::TouchpadButNoInputmodules => {
                self.bitmaps.latch_open_help_l().draw(buffer);
                self.bitmaps.latch_open_help_r().draw(buffer);

                draw_textbox(
                    buffer,
                    Point::new(WIDTH_I32 - 218 - 761, 1070),
                    TextColor::Info,
                    "1. Unlatch the Latches",
                );
                draw_textbox(
                    buffer,
                    Point::new(WIDTH_I32 - 218 - 761, 1150),
                    TextColor::Info,
                    "2. Unlatch the Latches",
                );
            }
            StateFrame::Finished => {
                self.bitmaps.latch_close_help_l().draw(buffer);
                self.bitmaps.latch_close_help_r().draw(buffer);

                draw_textbox(
                    buffer,
                    Point::new(1555, 1100),
                    TextColor::Info,
                    "Press in latch to lock",
                );
            }
            _ => {}
        }

        // Draw input module positions over laptop
        if matches!(
            self.frame,
            StateFrame::TouchpadButNoInputmodules
                | StateFrame::InputModulesMissing
                | StateFrame::NoTouchpad
                | StateFrame::Finished
        ) {
            self.draw_inputmodules(buffer, input_deck)
        }

        // Help text on the bottom
        match self.frame {
            StateFrame::NoExpansionBay => {
                draw_textbox(
                    buffer,
                    Point::new(620, HEIGHT_I32 - 96 - 75),
                    TextColor::Warn,
                    "Expansion Bay Module or Expansion Bay Cable not detected",
                );
            }
            StateFrame::NoHubboard => {
                draw_textbox(
                    buffer,
                    Point::new(620, HEIGHT_I32 - 96 - 75),
                    TextColor::Warn,
                    "Input Module Connector Board not detected",
                );
            }
            StateFrame::TouchpadButNoInputmodules => {
                draw_textbox(
                    buffer,
                    Point::new(620, HEIGHT_I32 - 96 - 75),
                    TextColor::Warn,
                    "Input Module not detected",
                );
            }
            StateFrame::InputModulesMissing => {
                draw_textbox(
                    buffer,
                    Point::new(620, HEIGHT_I32 - 96 - 75),
                    TextColor::Warn,
                    "Input Module not detected",
                );
            }
            StateFrame::NoTouchpad => {
                draw_textbox(
                    buffer,
                    Point::new(620, HEIGHT_I32 - 96 - 75),
                    TextColor::Warn,
                    "Touchpad not detected",
                );
            }
            StateFrame::Finished => {
                draw_textbox(
                    buffer,
                    Point::new(620, HEIGHT_I32 - 96 - 75),
                    TextColor::Ok,
                    "Input Modules and Touchpad detected",
                );
            }
        }
    }

    fn draw_inputmodules(&mut self, buffer: &mut Buffer, input_deck: &InputDeckStatus) {
        if input_deck.top_row.pos0 == InputModuleType::GenericC {
            self.bitmaps.c1_module().draw(buffer);
        }
        if input_deck.top_row.pos1 == InputModuleType::GenericC {
            self.bitmaps.c1_module().draw_pos(
                buffer,
                Point {
                    x: 329 + 98 - 1,
                    y: HEIGHT_I32 - 263 - 263 - 319,
                },
            );
        }
        if input_deck.top_row.pos3 == InputModuleType::GenericC {
            self.bitmaps.c1_module().draw_pos(
                buffer,
                Point {
                    x: 329 + 804 - 2,
                    y: HEIGHT_I32 - 263 - 263 - 319,
                },
            );
        }
        if input_deck.top_row.pos4 == InputModuleType::GenericC {
            self.bitmaps.c1_module().draw_pos(
                buffer,
                Point {
                    x: 329 + 804 + 98 - 3,
                    y: HEIGHT_I32 - 263 - 263 - 319,
                },
            );
        }

        if input_deck.top_row.pos0 == InputModuleType::GenericA
            || input_deck.top_row.pos0 == InputModuleType::KeyboardA
        {
            self.bitmaps.keyboard().draw_pos(
                buffer,
                Point {
                    x: 329,
                    y: HEIGHT_I32 - 263 - 263 - 319,
                },
            );
        }
        if input_deck.top_row.pos1 == InputModuleType::GenericA
            || input_deck.top_row.pos1 == InputModuleType::KeyboardA
        {
            self.bitmaps.keyboard().draw(buffer);
        }
        if input_deck.top_row.pos2 == InputModuleType::GenericA
            || input_deck.top_row.pos2 == InputModuleType::KeyboardA
        {
            self.bitmaps.keyboard().draw_pos(
                buffer,
                Point {
                    x: 329 + 97 * 2 - 1,
                    y: HEIGHT_I32 - 263 - 263 - 319,
                },
            );
        }

        if input_deck.top_row.pos0 == InputModuleType::GenericB
            || input_deck.top_row.pos0 == InputModuleType::KeyboardB
        {
            // TODO: Numpad image
            self.bitmaps.b1_module().draw(buffer);
        }
        if input_deck.top_row.pos3 == InputModuleType::GenericB
            || input_deck.top_row.pos3 == InputModuleType::KeyboardB
        {
            // TODO: Numpad image
            self.bitmaps.b1_module().draw_pos(
                buffer,
                Point {
                    x: 329 + 804 - 2,
                    y: HEIGHT_I32 - 263 - 263 - 319,
                },
            );
        }

        if input_deck.touchpad_present {
            self.bitmaps.touchpad().draw(buffer);
        }
    }
}

fn get_gop_buffer(bs: &BootServices) -> Option<(ScopedProtocol<GraphicsOutput>, Buffer)> {
    let gop_handles = if let Ok(gop_handles) =
        bs.locate_handle_buffer(SearchType::ByProtocol(&GraphicsOutput::GUID))
    {
        gop_handles
    } else {
        info!("GraphicsOutput protocol is missing");
        return None;
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
        return None;
    };

    let mode = gop.current_mode_info();
    let (width, height) = mode.resolution();
    let size = Point::new(width as i32, height as i32);
    info!("Current Resolution {}x{}", width, height);

    // Create a buffer to draw into.
    let (width, height) = gop.current_mode_info().resolution();
    Some((gop, Buffer::new(width, height)))
}

pub fn get_expansion_bay(ec: &Option<CrosEc>) -> EcResponseExpansionBayStatus {
    let emulated_bay = EcResponseExpansionBayStatus {
        state: (ExpansionByStates::ModuleEnabled as u8)
            | (ExpansionByStates::HatchSwitchClosed as u8),
        board_id_0: 0,
        board_id_1: 0,
    };

    #[cfg(not(feature = "qemu"))]
    let expansion_bay = EcRequestExpansionBayStatus {}.send_command(ec.as_ref().unwrap());
    #[cfg(feature = "qemu")]
    let expansion_bay: core::result::Result<EcResponseExpansionBayStatus, ()> =
        core::result::Result::Err(());
    if let Ok(expansion_bay) = expansion_bay {
        //println!("Got it!");
        expansion_bay
    } else {
        //println!("Emulated bay data");
        emulated_bay
    }
}

pub fn get_input_deck(ec: &Option<CrosEc>) -> InputDeckStatus {
    let emulated_deck = InputDeckStatus {
        state: InputDeckState::On,
        hubboard_present: false,
        touchpad_present: true,
        top_row: TopRowPositions {
            pos0: InputModuleType::GenericC,
            pos1: InputModuleType::GenericC,
            pos2: InputModuleType::GenericA,
            pos3: InputModuleType::Disconnected,
            pos4: InputModuleType::Disconnected,
        },
    };

    #[cfg(not(feature = "qemu"))]
    let input_deck = ec.as_ref().unwrap().get_input_deck_status();
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

fn draw_version(buffer: &mut Buffer) {
    // Place version information in the lower right corner
    let font = ascii::FONT_9X18_BOLD;
    let cs = Point::new(
        font.character_size.width as i32,
        font.character_size.height as i32,
    );
    let (height, ver_text) = if built_info::DEBUG {
        (
            4,
            format!(
                "Version: {}\nRef:     {}\nCommit:  {}\nDirty:   {}",
                built_info::PKG_VERSION,
                built_info::GIT_HEAD_REF.unwrap_or("?"),
                built_info::GIT_COMMIT_HASH.unwrap_or("?"),
                built_info::GIT_DIRTY.unwrap_or(false),
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
        (Point::new(WIDTH_I32 - 40 - 2 * 132 - 20, HEIGHT_I32 - 40 - 171)
            + Point::new(0, 132 - cs.y * height))
            - Point::new(cs.x * ver_len + cs.x, 0),
        MonoTextStyle::new(&font, Rgb888::WHITE),
    );
    text.draw(buffer).unwrap();
}

fn clear_screen(buffer: &mut Buffer, gop: &mut ScopedProtocol<GraphicsOutput>) -> uefi::Result {
    // Clear screen to black. Otherwise the next app is responsible for clearing.
    // If the app was launched from the UEFI shell, the user would have to manually clear
    let whole_screen: Rectangle =
        Rectangle::with_corners(Point::new(0, 0), Point::new(WIDTH_I32, HEIGHT_I32));
    let blank_style = PrimitiveStyleBuilder::new()
        .fill_color(Rgb888::BLACK)
        .build();
    whole_screen.into_styled(blank_style).draw(buffer).unwrap();
    buffer.blit(gop)?;

    Ok(())
}

pub fn start(bs: &BootServices, _st: &SystemTable<Boot>, ec: &Option<CrosEc>) -> uefi::Result {
    let input_deck = get_input_deck(ec);
    let expansion_bay = get_expansion_bay(ec);

    // If we start out as fully populated we want to boot as fast as possible
    // and skip the GUI
    if input_deck.fully_populated() && expansion_bay.module_enabled() {
        info!("Started out as fully populated, booting immediately");
        return Ok(());
    }

    let (mut gop, mut buffer) = if let Some((gop, buf)) = get_gop_buffer(bs) {
        (gop, buf)
    } else {
        // Something went wrong with GOP initialization, keep booting
        // TODO: Maybe in DVT2 we should still assert here?
        return Ok(());
    };

    let mut state = State::new();
    loop {
        let input_deck = get_input_deck(&ec);
        let expansion_bay = get_expansion_bay(ec);

        let need_redraw = Some(&input_deck) != state.prev_input_deck.as_ref();
        state.frame = state.next_state(&input_deck, &expansion_bay);

        state.draw_state(&mut buffer, &input_deck);
        draw_version(&mut buffer);
        buffer.blit(&mut gop)?;
        bs.stall(2_000_000);

        state.prev_input_deck = Some(input_deck);
    }
    clear_screen(&mut buffer, &mut gop);
    Ok(())
}
