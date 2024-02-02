use alloc::format;
use alloc::string::{String, ToString};
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
use framework_lib::power::ReducedPowerInfo;
#[allow(unused_imports)]
use log::{debug, error, info, trace};
use uefi::proto::console::gop::GraphicsOutput;
use uefi::table::boot::ScopedProtocol;
use zune_core::result::DecodingResult;
use zune_png::PngDecoder;

use crate::buffer::Buffer;
use crate::built_info;
use crate::state_machine::{DebugInfo, PD3_VER};

pub const WIDTH: usize = 2560;
pub const WIDTH_I32: i32 = 2560;
pub const HEIGHT: usize = 1600;
pub const HEIGHT_I32: i32 = 1600;
pub const BACKGROUND_COLOR: Rgb888 = Rgb888::new(0xF3, 0xF2, 0xF2);
pub const BLACK_BAR: Rgb888 = Rgb888::new(0x33, 0x33, 0x33);

pub const WARN_X: i32 = WIDTH_I32 / 2;
pub const WARN_Y: i32 = HEIGHT_I32 - 114 - 75;

#[derive(Default)]
pub struct StateBitmaps {
    qr_code_1: Option<Vec<u8>>,
    qr_code_2: Option<Vec<u8>>,
    qr_code_3: Option<Vec<u8>>,
    button_long_off: Option<Vec<u8>>,
    button_short_skip: Option<Vec<u8>>,
    button_short_reboot: Option<Vec<u8>>,
    button_boot: Option<Vec<u8>>,
    laptop_latches_open: Option<Vec<u8>>,
    latches_closed_left: Option<Vec<u8>>,
    latches_closed_right: Option<Vec<u8>>,
    touchpad: Option<Vec<u8>>,
    full_width: Option<Vec<u8>>,
    keyboard: Option<Vec<u8>>,
    b1_module: Option<Vec<u8>>,
    numpad: Option<Vec<u8>>,
    _macropad: Option<Vec<u8>>,
    c1_module: Option<Vec<u8>>,
    c2_module: Option<Vec<u8>>,
    hubboard_missing: Option<Vec<u8>>,
    expansion_bay_missing: Option<Vec<u8>>,
    latch_warning: Option<Vec<u8>>,
    fans_missing_text: Option<Vec<u8>>,
    interposer_closed_text: Option<Vec<u8>>,
    hubboard_missing_text: Option<Vec<u8>>,
    inputmodules_touchpad_ok_text: Option<Vec<u8>>,
    inputmodules_not_detected_text: Option<Vec<u8>>,
    interposer_door_open_text: Option<Vec<u8>>,
    left_fan_not_detected_text: Option<Vec<u8>>,
    module_missing_text: Option<Vec<u8>>,
    bay_bad_connection_text: Option<Vec<u8>>,
    right_fan_not_detected_text: Option<Vec<u8>>,
    touchpad_not_detected_text: Option<Vec<u8>>,
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
    pub fn qr_code_1(&mut self) -> ImagePlusMeta {
        if self.qr_code_1.is_none() {
            let png = include_bytes!("../resources/figma/qr-code-expansion-bay.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
                panic!("Failed to decode PNG")
            };
            self.qr_code_1 = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self.qr_code_1.as_ref().unwrap(),
            width: 168,
            pos: Point { x: 1814, y: 967 },
        }
    }
    pub fn qr_code_2(&mut self) -> ImagePlusMeta {
        if self.qr_code_2.is_none() {
            let png = include_bytes!("../resources/figma/qr-code-hubboard.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
                panic!("Failed to decode PNG")
            };
            self.qr_code_2 = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self.qr_code_2.as_ref().unwrap(),
            width: 168,
            pos: Point { x: 1814, y: 967 },
        }
    }
    pub fn qr_code_3(&mut self) -> ImagePlusMeta {
        if self.qr_code_3.is_none() {
            let png = include_bytes!("../resources/figma/qr-code-inputmodules.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
                panic!("Failed to decode PNG")
            };
            self.qr_code_3 = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self.qr_code_3.as_ref().unwrap(),
            width: 168,
            pos: Point { x: 1814, y: 967 },
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
            width: 148,
            pos: Point {
                x: WIDTH_I32 - 30 - 148,
                y: HEIGHT_I32 - 30 - 164,
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
            width: 148,
            pos: Point {
                x: WIDTH_I32 - 30 * 2 - 148 * 2,
                y: HEIGHT_I32 - 30 - 164,
            },
        }
    }

    pub fn button_short_reboot(&mut self) -> ImagePlusMeta {
        if self.button_short_reboot.is_none() {
            let png = include_bytes!("../resources/figma/button-short-reboot.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
                panic!("Failed to decode PNG")
            };
            self.button_short_reboot = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self.button_short_reboot.as_ref().unwrap(),
            width: 148,
            pos: Point {
                x: WIDTH_I32 - 30 * 2 - 148 * 2,
                y: HEIGHT_I32 - 30 - 164,
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
            width: 148,
            pos: Point {
                x: WIDTH_I32 - 30 - 148,
                y: HEIGHT_I32 - 30 - 164,
            },
        }
    }

    pub fn latches_closed_left(&mut self) -> ImagePlusMeta {
        if self.latches_closed_left.is_none() {
            let png = include_bytes!("../resources/figma/latch-closed-left.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
                panic!("Failed to decode PNG")
            };
            self.latches_closed_left = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self.latches_closed_left.as_ref().unwrap(),
            width: 24,
            pos: Point { x: 805, y: 1112 },
        }
    }

    pub fn latches_closed_right(&mut self) -> ImagePlusMeta {
        if self.latches_closed_right.is_none() {
            let png = include_bytes!("../resources/figma/latch-closed-right.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
                panic!("Failed to decode PNG")
            };
            self.latches_closed_right = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self.latches_closed_right.as_ref().unwrap(),
            width: 24,
            pos: Point { x: 1731, y: 1112 },
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
            assert_eq!(image_bytes.len() % 1176, 0);
            self.laptop_latches_open = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self.laptop_latches_open.as_ref().unwrap(),
            width: 1176,
            pos: Point { x: 692, y: 136 },
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
            width: 736,
            // Middle position
            pos: Point { x: 912, y: 874 },
        }
    }

    pub fn full_width(&mut self) -> ImagePlusMeta {
        if self.full_width.is_none() {
            let png = include_bytes!("../resources/figma/full-width-module.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
                panic!("Failed to decode PNG")
            };
            self.full_width = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self.full_width.as_ref().unwrap(),
            width: 904,
            pos: Point { x: 829, y: 583 },
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
            width: 736,
            // Middle position
            pos: Point {
                x: 829 + 84,
                y: 583,
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
            width: 168,
            // Top left position
            pos: Point { x: 829, y: 583 },
        }
    }

    pub fn numpad(&mut self) -> ImagePlusMeta {
        if self.numpad.is_none() {
            let png = include_bytes!("../resources/figma/numpad.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
                panic!("Failed to decode PNG")
            };
            self.numpad = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self.numpad.as_ref().unwrap(),
            width: 168,
            // Top left position
            pos: Point { x: 829, y: 583 },
        }
    }

    pub fn _macropad(&mut self) -> ImagePlusMeta {
        if self._macropad.is_none() {
            let png = include_bytes!("../resources/figma/macropad.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
                panic!("Failed to decode PNG")
            };
            self._macropad = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self._macropad.as_ref().unwrap(),
            width: 168,
            // Top left position
            pos: Point { x: 829, y: 583 },
        }
    }

    pub fn c1_module(&mut self) -> ImagePlusMeta {
        if self.c1_module.is_none() {
            let png = include_bytes!("../resources/figma/c1c2-black-module.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
                panic!("Failed to decode PNG")
            };
            self.c1_module = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self.c1_module.as_ref().unwrap(),
            width: 84,
            // Top left position
            pos: Point { x: 829, y: 583 },
        }
    }

    pub fn c2_module(&mut self) -> ImagePlusMeta {
        if self.c2_module.is_none() {
            let png = include_bytes!("../resources/figma/c1c2-silver-module.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
                panic!("Failed to decode PNG")
            };
            self.c2_module = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self.c2_module.as_ref().unwrap(),
            width: 84,
            // Bottom left position
            pos: Point { x: 829, y: 874 },
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
            width: 79,
            pos: Point { x: 1573, y: 781 },
        }
    }

    pub fn expansion_bay_missing(&mut self) -> ImagePlusMeta {
        if self.expansion_bay_missing.is_none() {
            let png = include_bytes!("../resources/figma/expansion-bay-missing.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
                panic!("Failed to decode PNG")
            };
            self.expansion_bay_missing = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self.expansion_bay_missing.as_ref().unwrap(),
            width: 916,
            pos: Point { x: 822, y: 477 },
        }
    }

    pub fn latch_warning(&mut self) -> ImagePlusMeta {
        if self.latch_warning.is_none() {
            let png = include_bytes!("../resources/figma/latch-warning.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
                panic!("Failed to decode PNG")
            };
            self.latch_warning = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self.latch_warning.as_ref().unwrap(),
            width: 188,
            pos: Point { x: 1181, y: 616 },
        }
    }
    pub fn fans_missing_text(&mut self) -> ImagePlusMeta {
        if self.fans_missing_text.is_none() {
            let png = include_bytes!("../resources/figma/warning-text/fans-missing.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
                panic!("Failed to decode PNG")
            };
            self.fans_missing_text = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self.fans_missing_text.as_ref().unwrap(),
            width: 336,
            pos: Point {
                x: WARN_X - 336 / 2,
                y: WARN_Y,
            },
        }
    }

    pub fn interposer_closed_text(&mut self) -> ImagePlusMeta {
        if self.interposer_closed_text.is_none() {
            let png = include_bytes!("../resources/figma/warning-text/interposer-closed.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
                panic!("Failed to decode PNG")
            };
            self.interposer_closed_text = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self.interposer_closed_text.as_ref().unwrap(),
            width: 795,
            pos: Point {
                x: WARN_X - 795 / 2,
                y: WARN_Y,
            },
        }
    }

    pub fn hubboard_missing_text(&mut self) -> ImagePlusMeta {
        if self.hubboard_missing_text.is_none() {
            let png = include_bytes!("../resources/figma/warning-text/hubboard-missing.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
                panic!("Failed to decode PNG")
            };
            self.hubboard_missing_text = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self.hubboard_missing_text.as_ref().unwrap(),
            width: 645,
            pos: Point {
                x: WARN_X - 645 / 2,
                y: WARN_Y,
            },
        }
    }

    pub fn inputmodules_touchpad_ok_text(&mut self) -> ImagePlusMeta {
        if self.inputmodules_touchpad_ok_text.is_none() {
            let png =
                include_bytes!("../resources/figma/warning-text/inputmodules-and-touchpad-ok.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
                panic!("Failed to decode PNG")
            };
            self.inputmodules_touchpad_ok_text = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self.inputmodules_touchpad_ok_text.as_ref().unwrap(),
            width: 577,
            pos: Point {
                x: WARN_X - 557 / 2,
                y: WARN_Y,
            },
        }
    }

    pub fn inputmodules_not_detected_text(&mut self) -> ImagePlusMeta {
        if self.inputmodules_not_detected_text.is_none() {
            let png =
                include_bytes!("../resources/figma/warning-text/inputmodules-not-detected.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
                panic!("Failed to decode PNG")
            };
            self.inputmodules_not_detected_text = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self.inputmodules_not_detected_text.as_ref().unwrap(),
            width: 453,
            pos: Point {
                x: WARN_X - 453 / 2,
                y: WARN_Y,
            },
        }
    }

    pub fn interposer_door_open_text(&mut self) -> ImagePlusMeta {
        if self.interposer_door_open_text.is_none() {
            let png = include_bytes!("../resources/figma/warning-text/interposer-door-open.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
                panic!("Failed to decode PNG")
            };
            self.interposer_door_open_text = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self.interposer_door_open_text.as_ref().unwrap(),
            width: 440,
            pos: Point {
                x: WARN_X - 440 / 2,
                y: WARN_Y,
            },
        }
    }

    pub fn left_fan_not_detected_text(&mut self) -> ImagePlusMeta {
        if self.left_fan_not_detected_text.is_none() {
            let png = include_bytes!("../resources/figma/warning-text/left-fan-not-detected.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
                panic!("Failed to decode PNG")
            };
            self.left_fan_not_detected_text = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self.left_fan_not_detected_text.as_ref().unwrap(),
            width: 376,
            pos: Point {
                x: WARN_X - 376 / 2,
                y: WARN_Y,
            },
        }
    }

    pub fn module_missing_text(&mut self) -> ImagePlusMeta {
        if self.module_missing_text.is_none() {
            let png = include_bytes!("../resources/figma/warning-text/module-missing.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
                panic!("Failed to decode PNG")
            };
            self.module_missing_text = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self.module_missing_text.as_ref().unwrap(),
            width: 546,
            pos: Point {
                x: WARN_X - 546 / 2,
                y: WARN_Y,
            },
        }
    }

    pub fn bay_bad_connection_text(&mut self) -> ImagePlusMeta {
        if self.bay_bad_connection_text.is_none() {
            let png = include_bytes!("../resources/figma/warning-text/bay-bad-connection.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
                panic!("Failed to decode PNG")
            };
            self.bay_bad_connection_text = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self.bay_bad_connection_text.as_ref().unwrap(),
            width: 631,
            pos: Point {
                x: WARN_X - 631 / 2,
                y: WARN_Y,
            },
        }
    }

    pub fn right_fan_not_detected_text(&mut self) -> ImagePlusMeta {
        if self.right_fan_not_detected_text.is_none() {
            let png = include_bytes!("../resources/figma/warning-text/right-fan-not-detected.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
                panic!("Failed to decode PNG")
            };
            self.right_fan_not_detected_text = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self.right_fan_not_detected_text.as_ref().unwrap(),
            width: 393,
            pos: Point {
                x: WARN_X - 393 / 2,
                y: WARN_Y,
            },
        }
    }

    pub fn touchpad_not_detected_text(&mut self) -> ImagePlusMeta {
        if self.touchpad_not_detected_text.is_none() {
            let png = include_bytes!("../resources/figma/warning-text/touchpad-not-detected.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
                panic!("Failed to decode PNG")
            };
            self.touchpad_not_detected_text = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self.touchpad_not_detected_text.as_ref().unwrap(),
            width: 395,
            pos: Point {
                x: WARN_X - 395 / 2,
                y: WARN_Y,
            },
        }
    }
}

// Text bubble colors
/// background.on-background
const BG_COLOR: Rgb888 = Rgb888::new(0x28, 0x25, 0x24);
const RED_WARN: Rgb888 = Rgb888::new(0xff, 0xe1, 0xe0);
const RED_BORDER: Rgb888 = Rgb888::new(0xca, 0x1c, 0x16);
/// warning.warning-10
const YELLOW_WARN: Rgb888 = Rgb888::new(0xff, 0xf6, 0xe0);
/// warning.warning-30
const YELLOW_BORDER: Rgb888 = Rgb888::new(0xff, 0xdb, 0x80);
/// success.on-success
const GREEN_OK: Rgb888 = Rgb888::new(0xf5, 0xff, 0xfb);
/// success.success
const GREEN_BORDER: Rgb888 = Rgb888::new(0x00, 0x8a, 0x4e);

pub enum TextColor {
    Error,
    Warn,
    _Ok,
    _Info,
}
impl TextColor {
    pub fn color(&self) -> (Rgb888, Rgb888, Rgb888) {
        match self {
            TextColor::Error => (BG_COLOR, RED_WARN, RED_BORDER),
            TextColor::Warn => (BG_COLOR, YELLOW_WARN, YELLOW_BORDER),
            TextColor::_Ok => (BG_COLOR, GREEN_OK, GREEN_BORDER),
            TextColor::_Info => (BG_COLOR, Rgb888::WHITE, Rgb888::BLACK),
        }
    }
}
const TEXT_OFFSET: i32 = 80;
const TEXT_PAD: u32 = 30;
const TEXT_YPAD: u32 = 45;
const TEXT_BORDER: u32 = 2;
const TEXT_LEFTPAD: i32 = (TEXT_PAD + TEXT_BORDER) as i32;
pub fn draw_textbox(buffer: &mut Buffer, pos: Point, color: TextColor, text_str: &str) {
    let font = ascii::FONT_9X18_BOLD;
    //let font = ascii::FONT_10X20;
    let (fg_color, bg_color, border_color) = color.color();

    // Move the text over to the left a bit, depending how long it is
    let middle = {
        let text = Text::new(
            text_str,
            pos + Point::new(TEXT_LEFTPAD, TEXT_OFFSET),
            MonoTextStyle::new(&font, fg_color),
        );
        TEXT_PAD + (text.bounding_box().size.width / 2)
    };

    let text = Text::new(
        text_str,
        pos + Point::new(TEXT_LEFTPAD, TEXT_OFFSET) - Point::new(middle as i32, 0),
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
        InputModuleType::FullWidth => "FW",
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

pub fn draw_battery_info(buffer: &mut Buffer, power_info: &Option<ReducedPowerInfo>) {
    if let Some(ReducedPowerInfo {
        battery: Some(ref battery),
        ac_present,
    }) = power_info
    {
        if battery.charge_percentage <= 20 {
            draw_textbox(
                buffer,
                Point::new(WIDTH as i32 / 2, 0),
                if *ac_present {
                    TextColor::Warn
                } else {
                    TextColor::Error
                },
                &format!(
                    "Battery is at {}% charge - {}",
                    battery.charge_percentage,
                    if *ac_present {
                        "Charging"
                    } else {
                        "Not charging"
                    }
                ),
            );
        }
    };
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
            7, // Need to update if adding more lines
            if let Some(debug_info) = debug_info {
                let power_str = if let Some(power_info) = debug_info.power_info {
                    if let Some(battery) = power_info.battery {
                        format!(
                            "AC Power: {}, Battery: Yes, Charge: {}%",
                            power_info.ac_present, battery.charge_percentage
                        )
                    } else {
                        format!("AC Power: {}, Battery:  No", power_info.ac_present)
                    }
                } else {
                    String::from("AC/DC:    ???")
                };
                let usb_fw_versions = debug_info
                    .usb_devices
                    .into_iter()
                    .filter(|dev| dev.is_framework_keyboard())
                    .map(|dev| {
                        let name = dev.get_name().unwrap_or("???");
                        format!("{}: {}", name, dev.format_bcd_device())
                    })
                    .collect::<Vec<String>>()
                    .join(", ");
                format!(
                    // TODO: Pad dirty to the end
                    "Version:  {}\nRef:      {}, Dirty:{}\nCommit:   {}\nHas EC:   {}, PwrBtn pressed:{}, PwrBtn Long press cnt:{}\nBay State:{}, En:{}, Fault:{}, Closed:{}\nBayModule:IDs: ({}, {}), {:?}\nHW Diag:  {}\n          {:#032b}\nDeckState:{:?}, Hubboard:{}, Touchpad:{}, {}\n{}\nPD 3 Ver: {}, Expected: {}\n{}",
                    built_info::PKG_VERSION,
                    built_info::GIT_HEAD_REF.unwrap_or("?"),
                    built_info::GIT_DIRTY.unwrap_or(false),
                    built_info::GIT_COMMIT_HASH.unwrap_or("?"),
                    // Basics
                    debug_info.power_button,
                    debug_info.ec_present,
                    debug_info.long_press,
                    // Bay State
                    debug_info.bay_state,
                    debug_info.bay_enabled,
                    debug_info.bay_fault,
                    debug_info.hatch_closed,
                    // Bay Module IDs
                    debug_info.gpu0,
                    debug_info.gpu1,
                    debug_info.bay_module,
                    // HW Diag
                    debug_info.hw_diag,
                    { debug_info.hw_diag.hw_diag },
                    // Input Deck
                    debug_info.deck.state,
                    debug_info.deck.hubboard_present,
                    debug_info.deck.touchpad_present,
                    top_row_string(&debug_info.deck.top_row),
                    power_str,
                    debug_info.pd3_ver.map(|x| format!("{}", x)).unwrap_or("Unknown".to_string()),
                    PD3_VER,
                    usb_fw_versions,
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
    // Next to the button icons
    let bottom_right = (Point::new(WIDTH_I32 - 30 * 2 - 2 * 148, HEIGHT_I32 - 40 - 171)
        + Point::new(0, 132 - cs.y * height))
        - Point::new(cs.x * ver_len + cs.x, 0);
    let top_left = Point::new(20, 20);
    let pos = if verbose { bottom_right } else { top_left };
    let text = Text::new(
        &ver_text,
        pos,
        MonoTextStyle::new(&font, if verbose { BACKGROUND_COLOR } else { BLACK_BAR }),
    );

    // Make sure the background is cleared if we only update the version
    let background = text.bounding_box();
    let blank_style = PrimitiveStyleBuilder::new()
        .fill_color(if verbose { BLACK_BAR } else { BACKGROUND_COLOR })
        .build();
    background.into_styled(blank_style).draw(buffer).unwrap();

    text.draw(buffer).unwrap();
}

pub fn draw_black_bar(buffer: &mut Buffer) {
    let tl = Point::new(0, (HEIGHT as i32) - 300);
    let br = Point::new(WIDTH as i32, HEIGHT as i32);
    let rect = Rectangle::with_corners(tl, br);
    let style: PrimitiveStyle<Rgb888> = PrimitiveStyleBuilder::new().fill_color(BLACK_BAR).build();
    rect.into_styled(style).draw(buffer).unwrap();
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
