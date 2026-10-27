use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use embedded_graphics::geometry::{AnchorPoint, Dimensions, Size};
use embedded_graphics::image::{Image, ImageRaw};
use embedded_graphics::mono_font::{ascii, MonoTextStyle};
use embedded_graphics::pixelcolor::Rgb888;
use embedded_graphics::prelude::{Point, Primitive, RgbColor, Transform};
use embedded_graphics::primitives::{
    Circle, Line, PrimitiveStyle, PrimitiveStyleBuilder, Rectangle, RoundedRectangle,
};
use embedded_graphics::text::Text;
use embedded_graphics::Drawable;
use framework_lib::chromium_ec::commands::*;
use framework_lib::power::{AccelData, LidAngle, ReducedPowerInfo};
#[allow(unused_imports)]
use log::{debug, error, info, trace};
use uefi::boot::ScopedProtocol;
use uefi::proto::console::gop::GraphicsOutput;
use zune_core::result::DecodingResult;
use zune_png::PngDecoder;

use crate::buffer::Buffer;
use crate::built_info;
#[cfg(not(feature = "12in"))]
use crate::state_machine::DebugInfo;
#[cfg(feature = "12in")]
use crate::state_machine_12in::DebugInfo;

pub const WIDTH: usize = 1920;
pub const WIDTH_I32: i32 = 1920;
pub const HEIGHT: usize = 1200;
pub const HEIGHT_I32: i32 = 1200;
pub const BACKGROUND_COLOR: Rgb888 = Rgb888::new(0xFF, 0xFF, 0xFF); //0xF3, 0xF2, 0xF2);
pub const BLACK_BAR: Rgb888 = Rgb888::new(0x33, 0x33, 0x33);

pub const WARN_X: i32 = 50 + WIDTH_I32 / 2;
pub const WARN_Y: i32 = HEIGHT_I32 - 275;

#[derive(Default)]
pub struct StateBitmaps {
    bare: Option<Vec<u8>>,
    audio_highlight: Option<Vec<u8>>,
    battery_top_highlight: Option<Vec<u8>>,
    battery_bottom_highlight: Option<Vec<u8>>,
    fan_highlight: Option<Vec<u8>>,
    pogo_highlight: Option<Vec<u8>>,
    pwrbtn_highlight: Option<Vec<u8>>,
    switch_highlight: Option<Vec<u8>>,
    flap_highlight: Option<Vec<u8>>,
}

pub struct ImagePlusMeta<'a> {
    pub data: &'a [u8],
    pub width: u32,
    pub pos: Point,
}

impl ImagePlusMeta<'_> {
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
    pub fn bare(&mut self) -> ImagePlusMeta {
        if self.bare.is_none() {
            let png = include_bytes!("../resources/framework12/assembled-open.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
                panic!("Failed to decode PNG")
            };
            self.bare = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self.bare.as_ref().unwrap(),
            width: 1000,
            pos: Point { x: 500, y: 50 },
        }
    }
    pub fn audio_highlight(&mut self) -> ImagePlusMeta {
        if self.audio_highlight.is_none() {
            let png = include_bytes!("../resources/framework12/audio-highlight.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
                panic!("Failed to decode PNG")
            };
            self.audio_highlight = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self.audio_highlight.as_ref().unwrap(),
            width: 152,
            pos: Point { x: 509, y: 557 },
        }
    }
    pub fn battery_top_highlight(&mut self) -> ImagePlusMeta {
        if self.battery_top_highlight.is_none() {
            let png = include_bytes!("../resources/framework12/battery-top-highlight.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
                panic!("Failed to decode PNG")
            };
            self.battery_top_highlight = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self.battery_top_highlight.as_ref().unwrap(),
            width: 182,
            pos: Point { x: 765, y: 570 },
        }
    }
    pub fn battery_bottom_highlight(&mut self) -> ImagePlusMeta {
        if self.battery_bottom_highlight.is_none() {
            let png = include_bytes!("../resources/framework12/battery-bottom-highlight.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
                panic!("Failed to decode PNG")
            };
            self.battery_bottom_highlight = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self.battery_bottom_highlight.as_ref().unwrap(),
            width: 774,
            pos: Point { x: 613, y: 633 },
        }
    }
    pub fn fan_highlight(&mut self) -> ImagePlusMeta {
        if self.fan_highlight.is_none() {
            let png = include_bytes!("../resources/framework12/fan-highlight.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
                panic!("Failed to decode PNG")
            };
            self.fan_highlight = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self.fan_highlight.as_ref().unwrap(),
            width: 261,
            pos: Point { x: 686, y: 292 },
        }
    }
    pub fn pogo_highlight(&mut self) -> ImagePlusMeta {
        if self.pogo_highlight.is_none() {
            let png = include_bytes!("../resources/framework12/pogo-highlight.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
                panic!("Failed to decode PNG")
            };
            self.pogo_highlight = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self.pogo_highlight.as_ref().unwrap(),
            width: 199,
            pos: Point { x: 944, y: 595 },
        }
    }
    pub fn pwrbtn_highlight(&mut self) -> ImagePlusMeta {
        if self.pwrbtn_highlight.is_none() {
            let png = include_bytes!("../resources/framework12/powerbutton-highlight.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
                panic!("Failed to decode PNG")
            };
            self.pwrbtn_highlight = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self.pwrbtn_highlight.as_ref().unwrap(),
            width: 138,
            pos: Point { x: 1341, y: 556 },
        }
    }
    pub fn switch_highlight(&mut self) -> ImagePlusMeta {
        if self.switch_highlight.is_none() {
            let png = include_bytes!("../resources/framework12/switch-highlight.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
                panic!("Failed to decode PNG")
            };
            self.switch_highlight = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self.switch_highlight.as_ref().unwrap(),
            width: 24,
            pos: Point { x: 1319, y: 539 },
        }
    }
    pub fn flap_highlight(&mut self) -> ImagePlusMeta {
        if self.flap_highlight.is_none() {
            let png = include_bytes!("../resources/framework12/flap-highlight.png");
            let mut decoder = PngDecoder::new(png);
            let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
                panic!("Failed to decode PNG")
            };
            self.flap_highlight = Some(image_bytes);
        }

        ImagePlusMeta {
            // Guaranteed to be set above
            data: self.flap_highlight.as_ref().unwrap(),
            width: 263,
            pos: Point { x: 962, y: 472 },
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
    Ok,
    _Info,
}
impl TextColor {
    pub fn color(&self) -> (Rgb888, Rgb888, Rgb888) {
        match self {
            TextColor::Error => (BG_COLOR, RED_WARN, RED_BORDER),
            TextColor::Warn => (BG_COLOR, YELLOW_WARN, YELLOW_BORDER),
            TextColor::Ok => (BG_COLOR, GREEN_OK, GREEN_BORDER),
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
    let rect = RoundedRectangle::with_equal_corners(
        rect,
        Size::new(rect.size.height / 5, rect.size.height / 5),
    );
    let blank_style: PrimitiveStyle<Rgb888> = PrimitiveStyleBuilder::new()
        .fill_color(bg_color)
        .stroke_width(TEXT_BORDER)
        .stroke_color(border_color)
        .build();
    rect.into_styled(blank_style).draw(buffer).unwrap();

    text.draw(buffer).unwrap();
}

// TODO: Update
pub fn draw_battery_info(buffer: &mut Buffer, power_info: &Option<ReducedPowerInfo>) {
    if let Some(ReducedPowerInfo {
        battery: Some(ref battery),
        ac_present,
    }) = power_info
    {
        if battery.charge_percentage <= 20 {
            draw_textbox(
                buffer,
                Point::new(50 + WIDTH as i32 / 2, 700),
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

pub fn draw_lid_angle(buffer: &mut Buffer, angle: LidAngle, pos: Point) {
    // Base
    Line::new(Point::new(0, 0), Point::new(100, 0))
        .translate(pos)
        .into_styled(PrimitiveStyle::with_stroke(BLACK_BAR, 15))
        .draw(buffer)
        .unwrap();

    let angle = if let LidAngle::Angle(deg) = angle {
        deg
    } else {
        // Can't draw lid
        return;
    };

    use micromath::F32Ext;
    let angle = angle as f32;
    let angle = angle.to_radians();

    // Lid
    let rotated = Point::new(
        (100_f32 - 100_f32 * f32::cos(angle)) as i32,
        (-100_f32 * f32::sin(angle)) as i32,
    );
    Line::new(rotated, Point::new(100, 0))
        .translate(pos)
        .into_styled(PrimitiveStyle::with_stroke(BLACK_BAR, 10))
        .draw(buffer)
        .unwrap();

    // Draw circles at the end of each line to make it look prettier
    Circle::with_center(pos, 15)
        .into_styled(PrimitiveStyle::with_fill(BLACK_BAR))
        .draw(buffer)
        .unwrap();
    Circle::with_center(pos + Point::new(100, 0), 15)
        .into_styled(PrimitiveStyle::with_fill(BLACK_BAR))
        .draw(buffer)
        .unwrap();
    Circle::with_center(pos + rotated, 10)
        .into_styled(PrimitiveStyle::with_fill(BLACK_BAR))
        .draw(buffer)
        .unwrap();
}

pub fn draw_accel_bubble(buffer: &mut Buffer, accel_reading: &AccelData, pos: Point) {
    let outer_diameter = 200;
    // Maximum value is 2.pow(16) / 4
    // Outer diameter is 200
    // Outer radius is 200 / 2
    let reading_scale = 2_i32.pow(16) / 4 / 200 * 2;
    // Reverse y direction because G-sensor's origin is in bottom left corrner
    // But screen drawing origin is on top left corner
    let reading_x = accel_reading.x as i32;
    let reading_y = -accel_reading.y as i32;
    let bubble_pos = pos + Point::new(reading_x / reading_scale, reading_y / reading_scale);

    // Inner circle
    Circle::with_center(pos, 100)
        .into_styled(PrimitiveStyle::with_stroke(BLACK_BAR, 2))
        .draw(buffer)
        .unwrap();
    // Outer circle
    Circle::with_center(pos, 200)
        .into_styled(PrimitiveStyle::with_stroke(BLACK_BAR, 2))
        .draw(buffer)
        .unwrap();

    // Indicator
    let style = if accel_reading.z >= 0 {
        PrimitiveStyle::with_fill(BLACK_BAR)
    } else {
        PrimitiveStyle::with_stroke(BLACK_BAR, 2)
    };
    Circle::with_center(bubble_pos, 50)
        .into_styled(style)
        .draw(buffer)
        .unwrap();
}

// TODO: Update
pub fn draw_version(buffer: &mut Buffer, debug_info: Option<DebugInfo>, verbose: bool) {
    // Place version information in the lower right corner
    let font = ascii::FONT_9X18_BOLD;
    let cs = Point::new(
        font.character_size.width as i32,
        font.character_size.height as i32,
    );
    debug!("Draw version: verbose={:?}", verbose);
    let (height, ver_text) = if verbose {
        (
            5,
            if let Some(debug_info) = debug_info {
                let power_str = if let Some(power_info) = debug_info.power_info {
                    if let Some(battery) = power_info.battery {
                        format!(
                            "AC Power:   {}, Battery: Yes, Charge: {}%",
                            power_info.ac_present, battery.charge_percentage
                        )
                    } else {
                        format!("AC Power:   {}, Battery:  No", power_info.ac_present)
                    }
                } else {
                    String::from("AC/DC:      ???")
                };
                format!(
                    // TODO: Pad dirty to the end
                    "Version:    {} \nRef:        {}, Dirty:{}\nCommit:     {}\nHas EC:     {}, PwrBtn pressed:  {}, PwrBtn Long press count:{}\nHW Diag:    {:#032b}\nCPU Fan:    {:?}\nAudioBoard: {:?} (Board ID: {:?})\nTouchpad:   {:?} (Board ID: {:?})\nChassisClsd:{:?}\nPwrBtnBoard:{:?}\nLid Accel:  ({:>6}, {:>6}, {:>6})\nBase Accel: ({:>6}, {:>6}, {:>6})\nLid Angle:  {}\n{}\nResolution: {}x{}",
                    built_info::PKG_VERSION,
                    built_info::GIT_HEAD_REF.unwrap_or("?"),
                    built_info::GIT_DIRTY.unwrap_or(false),
                    built_info::GIT_COMMIT_HASH.unwrap_or("?"),
                    // Basics
                    debug_info.ec_present,
                    debug_info.power_button,
                    debug_info.long_press,
                    // HW Diag
                    { debug_info.hw_diag.hw_diag },
                    // Input Deck
                    debug_info.hw_diag.hw_diag & (1 << DIAGNOSTICS_NOFAN) == 0,
                    debug_info.hw_diag.hw_diag & (1 << DIAGNOSTICS_AUDIO_DAUGHTERBOARD) == 0,
                    debug_info.deck.audioboard_present,
                    debug_info.hw_diag.hw_diag & (1 << DIAGNOSTICS_TOUCHPAD) == 0,
                    debug_info.deck.touchpad_present,
                    debug_info.deck.chassis_closed,
                    debug_info.deck.pwrbtn_present,
                    debug_info.accel_reading.0.x,
                    debug_info.accel_reading.0.y,
                    debug_info.accel_reading.0.z,
                    debug_info.accel_reading.1.x,
                    debug_info.accel_reading.1.y,
                    debug_info.accel_reading.1.z,
                    // TODO: Add temps
                    debug_info.accel_reading.2,
                    power_str,
                    debug_info.resolution.0,
                    debug_info.resolution.1,
                )
            } else {
                format!(
                    "Version: dbg{}\nRef:     {}\nCommit:  {}\nDirty:   {}",
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
    let top_left = Point::new(20, 20);
    // TODO: It gets overridden by background if I put it at bottom right, need to adjust that
    let bottom_right = top_left;
    let pos = if verbose { bottom_right } else { top_left };
    let text = Text::new(&ver_text, pos, MonoTextStyle::new(&font, BLACK_BAR));

    // Make sure the background is cleared if we only update the version
    let background = text.bounding_box();
    let blank_style = PrimitiveStyleBuilder::new()
        .fill_color(BACKGROUND_COLOR)
        .build();
    background.into_styled(blank_style).draw(buffer).unwrap();

    text.draw(buffer).unwrap();
}

pub fn draw_warning_box(buffer: &mut Buffer, pos: Point, size: Size) {
    let rect = Rectangle::new(pos, size);
    let style: PrimitiveStyle<Rgb888> = PrimitiveStyleBuilder::new()
        //.fill_color(RED_WARN)
        .stroke_width(10)
        .stroke_color(RED_BORDER)
        .build();
    rect.into_styled(style).draw(buffer).unwrap();
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
    let _ = buffer.clear(0x00, 0x00, 0x00);
    buffer.blit(gop)
}
