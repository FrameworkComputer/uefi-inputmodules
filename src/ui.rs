//! Drawing primitives shared by all platforms
use alloc::format;
use alloc::vec::Vec;
use embedded_graphics::geometry::{AnchorPoint, Dimensions, Size};
use embedded_graphics::image::{Image, ImageRaw};
use embedded_graphics::mono_font::{ascii, MonoTextStyle};
use embedded_graphics::pixelcolor::Rgb888;
use embedded_graphics::prelude::{Point, Primitive, RgbColor};
use embedded_graphics::primitives::{PrimitiveStyle, PrimitiveStyleBuilder, RoundedRectangle};
use embedded_graphics::text::Text;
use embedded_graphics::Drawable;
use framework_lib::power::ReducedPowerInfo;
use uefi::boot::ScopedProtocol;
use uefi::proto::console::gop::GraphicsOutput;
use zune_core::result::DecodingResult;
use zune_png::PngDecoder;

use crate::buffer::Buffer;

/// Decode an embedded PNG into raw RGB888 bytes
pub fn decode_png(png: &[u8]) -> Vec<u8> {
    let mut decoder = PngDecoder::new(png);
    let DecodingResult::U8(image_bytes) = decoder.decode().unwrap() else {
        panic!("Failed to decode PNG")
    };
    image_bytes
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

/// Show a warning about low battery, centered on `pos`
pub fn draw_battery_info(buffer: &mut Buffer, pos: Point, power_info: &Option<ReducedPowerInfo>) {
    if let Some(ReducedPowerInfo {
        battery: Some(ref battery),
        ac_present,
    }) = power_info
    {
        if battery.charge_percentage <= 20 {
            draw_textbox(
                buffer,
                pos,
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

pub fn clear_screen(buffer: &mut Buffer, gop: &mut ScopedProtocol<GraphicsOutput>) -> uefi::Result {
    // Clear screen to black. Otherwise the next app is responsible for clearing.
    // If the app was launched from the UEFI shell, the user would have to manually clear
    let whole_screen = buffer.bounding_box();
    let blank_style = PrimitiveStyleBuilder::new()
        .fill_color(Rgb888::BLACK)
        .build();
    whole_screen.into_styled(blank_style).draw(buffer).unwrap();
    buffer.blit(gop)?;

    Ok(())
}
