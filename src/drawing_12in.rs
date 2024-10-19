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
use framework_lib::chromium_ec::commands::*;
use framework_lib::power::{ReducedPowerInfo, LID_ANGLE_UNRELIABLE};
#[allow(unused_imports)]
use log::{debug, error, info, trace};
use uefi::proto::console::gop::GraphicsOutput;
use uefi::table::boot::ScopedProtocol;
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
pub const BACKGROUND_COLOR: Rgb888 = Rgb888::new(0xF3, 0xF2, 0xF2);
pub const BLACK_BAR: Rgb888 = Rgb888::new(0x33, 0x33, 0x33);

pub const WARN_X: i32 = WIDTH_I32 / 2;
pub const WARN_Y: i32 = HEIGHT_I32 - 114 - 75;

#[derive(Default)]
pub struct StateBitmaps {
    // TODO: Add bitmaps
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
    // TODO: Add bitmap drawing functions
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
            4,
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
                let lid_angle = if debug_info.accel_reading.2 == LID_ANGLE_UNRELIABLE {
                    "Unreliable".to_string()
                } else {
                    format!("{} deg", debug_info.accel_reading.2)
                };
                format!(
                    // TODO: Pad dirty to the end
                    "Version:    {} \nRef:        {}, Dirty:{}\nCommit:     {}\nHas EC:     {}, PwrBtn pressed:  {}, PwrBtn Long press count:{}\nHW Diag:    {:#032b}\nCPU Fan:    {:?}\nAudioBoard: {:?}\nDeck State: {:?}\nLid Accel:  ({:05}, {:05}, {:05})\nBase Accel: ({:05}, {:05}, {:05})\nLid Angle:  {}\n{}",
                    built_info::PKG_VERSION,
                    built_info::GIT_HEAD_REF.unwrap_or("?"),
                    built_info::GIT_DIRTY.unwrap_or(false),
                    built_info::GIT_COMMIT_HASH.unwrap_or("?"),
                    // Basics
                    debug_info.power_button,
                    debug_info.ec_present,
                    debug_info.long_press,
                    // HW Diag
                    // TODO: Need to decode for non-lotus
                    // debug_info.hw_diag,
                    { debug_info.hw_diag.hw_diag },
                    // Input Deck
                    debug_info.hw_diag.hw_diag & (1 << DIAGNOSTICS_NOFAN) == 0,
                    debug_info.hw_diag.hw_diag & (1 << DIAGNOSTICS_AUDIO_DAUGHTERBOARD) == 0,
                    debug_info.deck.touchpad_present,
                    debug_info.accel_reading.0.x,
                    debug_info.accel_reading.0.y,
                    debug_info.accel_reading.0.z,
                    debug_info.accel_reading.1.x,
                    debug_info.accel_reading.1.z,
                    debug_info.accel_reading.1.z,
                    // TODO: Add temps
                    lid_angle,
                    power_str
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
    let text = Text::new(
        &ver_text,
        pos,
        MonoTextStyle::new(&font, BLACK_BAR),
    );

    // Make sure the background is cleared if we only update the version
    let background = text.bounding_box();
    let blank_style = PrimitiveStyleBuilder::new()
        .fill_color(BACKGROUND_COLOR)
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
