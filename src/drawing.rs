use alloc::format;
use alloc::string::String;
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
use framework_lib::chromium_ec::input_deck::{InputModuleType, TopRowPositions};
use framework_lib::power::PowerInfo;
#[allow(unused_imports)]
use log::{debug, error, info, trace};
use uefi::proto::console::gop::GraphicsOutput;
use uefi::table::boot::ScopedProtocol;
use zune_core::result::DecodingResult;
use zune_png::PngDecoder;

use crate::buffer::Buffer;
use crate::built_info;
use crate::state_machine::DebugInfo;

pub const WIDTH: usize = 2560;
pub const WIDTH_I32: i32 = 2560;
pub const HEIGHT: usize = 1600;
pub const HEIGHT_I32: i32 = 1600;
pub const BACKGROUND_COLOR: Rgb888 = Rgb888::new(0x56, 0x76, 0x76);

#[derive(Default)]
pub struct StateBitmaps {
    qr_code_left: Option<Vec<u8>>,
    button_long_off: Option<Vec<u8>>,
    button_short_skip: Option<Vec<u8>>,
    button_boot: Option<Vec<u8>>,
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

pub struct ImagePlusMeta<'a> {
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
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
                panic!("Failed to decode PNG")
            };
            self.qr_code_left = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self.qr_code_left.as_ref().unwrap(),
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
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
                panic!("Failed to decode PNG")
            };
            self.button_long_off = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self.button_long_off.as_ref().unwrap(),
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
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
                panic!("Failed to decode PNG")
            };
            self.button_short_skip = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self.button_short_skip.as_ref().unwrap(),
            width: 132,
            pos: Point {
                x: WIDTH_I32 - 40 - 2 * 132 - 20,
                y: HEIGHT_I32 - 40 - 171,
            },
        }
    }

    pub fn button_boot(&mut self) -> ImagePlusMeta {
        if self.button_boot.is_none() {
            let png = include_bytes!("../resources/figma/button-boot.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
                panic!("Failed to decode PNG")
            };
            self.button_boot = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self.button_boot.as_ref().unwrap(),
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
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
                panic!("Failed to decode PNG")
            };
            self.laptop_latches_closed = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self.laptop_latches_closed.as_ref().unwrap(),
            width: 1296,
            pos: Point { x: 180, y: 263 },
        }
    }

    pub fn laptop_latches_open(&mut self) -> ImagePlusMeta {
        if self.laptop_latches_open.is_none() {
            let png = include_bytes!("../resources/figma/laptop-latch-open.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
                panic!("Failed to decode PNG")
            };
            // TODO: Add more of this assertion
            assert_eq!(image_bytes.len() % 1296, 0);
            self.laptop_latches_open = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self.laptop_latches_open.as_ref().unwrap(),
            width: 1296,
            pos: Point { x: 180, y: 263 },
        }
    }

    pub fn no_expansion_bay_instr(&mut self) -> ImagePlusMeta {
        if self.no_expansion_bay_instr.is_none() {
            let png = include_bytes!("../resources/figma/no-expansion-bay-instructions.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
                panic!("Failed to decode PNG")
            };
            self.no_expansion_bay_instr = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self.no_expansion_bay_instr.as_ref().unwrap(),
            width: 761,
            pos: Point {
                x: WIDTH_I32 - 218 - 761,
                // TODO: Instead of moving this up 30, we should move everything up 30 (except QR and buttons)
                // Otherwise the laptop doesn't lign up with this
                y: 264 - 30,
            },
        }
    }

    pub fn no_hubboard_instr(&mut self) -> ImagePlusMeta {
        if self.no_hubboard_instr.is_none() {
            let png = include_bytes!("../resources/figma/no-hubboard-instructions.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
                panic!("Failed to decode PNG")
            };
            self.no_hubboard_instr = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self.no_hubboard_instr.as_ref().unwrap(),
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
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
                panic!("Failed to decode PNG")
            };
            self.touchpad_no_modules_instr = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self.touchpad_no_modules_instr.as_ref().unwrap(),
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
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
                panic!("Failed to decode PNG")
            };
            self.inputmodules_missing_instr = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self.inputmodules_missing_instr.as_ref().unwrap(),
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
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
                panic!("Failed to decode PNG")
            };
            self.no_touchpad_instr = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self.no_touchpad_instr.as_ref().unwrap(),
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
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
                panic!("Failed to decode PNG")
            };
            self.latch_open_help_l = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self.latch_open_help_l.as_ref().unwrap(),
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
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
                panic!("Failed to decode PNG")
            };
            self.latch_open_help_r = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self.latch_open_help_r.as_ref().unwrap(),
            width: 181,
            pos: Point { x: 1364, y: 1091 },
        }
    }

    pub fn latch_close_help_l(&mut self) -> ImagePlusMeta {
        if self.latch_close_help_l.is_none() {
            let png = include_bytes!("../resources/figma/latch-close-help-l.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
                panic!("Failed to decode PNG")
            };
            self.latch_close_help_l = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self.latch_close_help_l.as_ref().unwrap(),
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
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
                panic!("Failed to decode PNG")
            };
            self.latch_close_help_r = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self.latch_close_help_r.as_ref().unwrap(),
            width: 180,
            pos: Point { x: 1355, y: 1091 },
        }
    }

    pub fn touchpad(&mut self) -> ImagePlusMeta {
        if self.touchpad.is_none() {
            let png = include_bytes!("../resources/figma/touchpad.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
                panic!("Failed to decode PNG")
            };
            self.touchpad = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self.touchpad.as_ref().unwrap(),
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
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
                panic!("Failed to decode PNG")
            };
            self.keyboard = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self.keyboard.as_ref().unwrap(),
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
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
                panic!("Failed to decode PNG")
            };
            self.b1_module = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self.b1_module.as_ref().unwrap(),
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
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
                panic!("Failed to decode PNG")
            };
            self.c1_module = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self.c1_module.as_ref().unwrap(),
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
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
                panic!("Failed to decode PNG")
            };
            self.hubboard_missing = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self.hubboard_missing.as_ref().unwrap(),
            width: 87,
            pos: Point { x: 180, y: 263 } + Point { x: 978, y: 715 },
        }
    }

    pub fn expansion_bay_populated(&mut self) -> ImagePlusMeta {
        if self.expansion_bay_populated.is_none() {
            let png = include_bytes!("../resources/figma/expansion-bay-installed.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
                panic!("Failed to decode PNG")
            };
            self.expansion_bay_populated = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self.expansion_bay_populated.as_ref().unwrap(),
            width: 1007,
            pos: Point { x: 180, y: 263 } + Point { x: 144, y: 377 },
        }
    }
}

const BG_COLOR: Rgb888 = Rgb888::new(0x28, 0x25, 0x24);
const RED_WARN: Rgb888 = Rgb888::new(0xff, 0xe1, 0xe0);
const RED_BORDER: Rgb888 = Rgb888::new(0xca, 0x1c, 0x16);
const GREEN_OK: Rgb888 = Rgb888::new(0xf5, 0xff, 0xfb);
const GREEN_BORDER: Rgb888 = Rgb888::new(0x00, 0x8a, 0x4e);

pub enum TextColor {
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
pub fn draw_textbox(buffer: &mut Buffer, pos: Point, color: TextColor, text: &str) {
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

fn mod_name(module: &InputModuleType) -> &str {
    match module {
        InputModuleType::GenericA => "A1",
        InputModuleType::GenericB => "B1",
        InputModuleType::GenericC => "C1",
        InputModuleType::KeyboardB => "NP",
        InputModuleType::KeyboardA => "KB",
        InputModuleType::Touchpad => "TP",
        InputModuleType::Disconnected => "No",
        _ => "??",
    }
}

fn top_row_string(top_row: &TopRowPositions) -> String {
    format!(
        "{}-{}-{}----{}-{}",
        mod_name(&top_row.pos0),
        mod_name(&top_row.pos1),
        mod_name(&top_row.pos2),
        mod_name(&top_row.pos3),
        mod_name(&top_row.pos4),
    )
}

pub fn draw_version(buffer: &mut Buffer, debug_info: Option<DebugInfo>, verbose: bool) {
    // Place version information in the lower right corner
    let font = ascii::FONT_9X18_BOLD;
    let cs = Point::new(
        font.character_size.width as i32,
        font.character_size.height as i32,
    );
    let (height, ver_text) = if verbose {
        (
            4,
            if let Some(debug_info) = debug_info {
                let power_str = if let Some(power_info) = debug_info.power_info {
                    if let Some(battery) = power_info.battery {
                        format!(
                            "AC: {}, Battery: Yes, Charge: {}%",
                            power_info.ac_present, battery.charge_percentage
                        )
                    } else {
                        format!("AC: {}, Battery:  No", power_info.ac_present)
                    }
                } else {
                    String::from("???")
                };
                format!(
                    "Version: {}\nRef:     {}\nCommit:  {}\nDirty:   {}\nB:{}, EC:{}, Long:{}\nBay State:{}, GPU0:{}, GPU1: {}, En:{}, Fault:{}\nHW Diag: {:#032b}\nDeckState:{:?}, Hub:{}, TP:{}, {}\n{}",
                    built_info::PKG_VERSION,
                    built_info::GIT_HEAD_REF.unwrap_or("?"),
                    built_info::GIT_COMMIT_HASH.unwrap_or("?"),
                    built_info::GIT_DIRTY.unwrap_or(false),
                    // Basics
                    debug_info.power_button,
                    debug_info.ec_present,
                    debug_info.long_press,
                    // Bay
                    debug_info.bay_state,
                    debug_info.gpu0,
                    debug_info.gpu1,
                    debug_info.bay_enabled,
                    debug_info.bay_fault,
                    // HW Diag
                    debug_info.hw_diag,
                    // Input Deck
                    debug_info.deck.state,
                    debug_info.deck.hubboard_present,
                    debug_info.deck.touchpad_present,
                    top_row_string(&debug_info.deck.top_row),
                    power_str
                )
            } else {
                format!(
                    "Version: {}\nRef:     {}\nCommit:  {}\nDirty:   {}",
                    built_info::PKG_VERSION,
                    built_info::GIT_HEAD_REF.unwrap_or("?"),
                    built_info::GIT_COMMIT_HASH.unwrap_or("?"),
                    built_info::GIT_DIRTY.unwrap_or(false),
                )
            },
        )
    } else {
        let power_str = if let Some(DebugInfo {
            power_info:
                Some(PowerInfo {
                    battery: Some(battery),
                    ..
                }),
            ..
        }) = debug_info
        {
            if battery.charge_percentage <= 20 {
                format!("\nBattery Level: {}%!", battery.charge_percentage)
            } else {
                String::new()
            }
        } else {
            String::new()
        };
        (1, format!("v{}{}", built_info::PKG_VERSION, power_str))
    };
    let ver_len = ver_text.lines().fold(0, |longest, cur| -> i32 {
        let cur_len = cur.len() as i32;
        if longest > cur_len {
            longest
        } else {
            cur_len
        }
    });
    // Next to the button icons
    let bottom_right = (Point::new(WIDTH_I32 - 40 - 2 * 132 - 20, HEIGHT_I32 - 40 - 171)
        + Point::new(0, 132 - cs.y * height))
        - Point::new(cs.x * ver_len + cs.x, 0);
    let top_left = Point::new(20, 20);
    let pos = if verbose { bottom_right } else { top_left };
    let text = Text::new(&ver_text, pos, MonoTextStyle::new(&font, Rgb888::WHITE));

    // Make sure the background is cleared if we only update the version
    let background = text.bounding_box();
    let blank_style = PrimitiveStyleBuilder::new()
        .fill_color(BACKGROUND_COLOR)
        .build();
    background.into_styled(blank_style).draw(buffer).unwrap();

    text.draw(buffer).unwrap();
}

pub fn clear_screen(buffer: &mut Buffer, gop: &mut ScopedProtocol<GraphicsOutput>) -> uefi::Result {
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
