//! WIP Implementation of the HII_IMAGE_DECODER protocol
//!
//! Currently I haven't been able to make it work. QEMU and laptop FW does not
//! seem to have this protocol installed. And I don't see where in EDK2 code
//! that would happen.
use core::ffi::c_void;
use core::ptr;
use uefi::proto::unsafe_protocol;
use uefi::{guid, Guid, Status};

// use alloc::string::String;
// use alloc::string::ToString;
// use alloc::vec;
// use alloc::vec::Vec;

const HII_IMAGE_DECODER_NAME_JPEG_GUID: Guid = guid!("efefd093-0d9b-46eb-a856-48350700c908");

const HII_IMAGE_DECODER_NAME_PNG_GUID: Guid = guid!("af060190-5e3a-4025-afbd-e1f905bfaa4c");

#[derive(Debug, Clone, Copy)]
pub enum DecoderName {
    Jpeg,
    Png,
    Other(Guid),
    // TODO: Remove this one
    Empty,
}
impl From<Guid> for DecoderName {
    fn from(guid: Guid) -> Self {
        if guid == HII_IMAGE_DECODER_NAME_JPEG_GUID {
            return DecoderName::Jpeg;
        }
        if guid == HII_IMAGE_DECODER_NAME_PNG_GUID {
            return DecoderName::Png;
        }
        DecoderName::Other(guid)
    }
}

#[repr(C)]
pub enum ColorType {
    Rgb = 0x0,
    Rgba = 0x1,
    Cmyk = 0x2,
    Unknown = 0xFF,
}

#[repr(C)]
pub struct ImageInfoHeader {
    pub decoder_name: Guid,
    pub image_info_size: u16,
    pub image_width: u16,
    pub image_height: u16,
    pub color_type: ColorType,
    pub color_depth_in_bits: u8,
}

#[repr(C)]
pub enum ImageInfo {
    Jpeg(JpegImageInfo),
    Png(PngImageInfo),
    Other(OtherImageInfo),
}

#[repr(C)]
pub struct JpegImageInfo {
    pub header: ImageInfoHeader,
    pub scan_type: u16,
    _reserved: u64,
}

#[repr(C)]
pub struct PngImageInfo {
    pub header: ImageInfoHeader,
    pub channels: u16,
    _reserved: u64,
}

#[repr(C)]
pub struct OtherImageInfo {
    pub header: ImageInfoHeader,
    pub file_extension: [u16; 1],
}

pub struct ImageOutput {
    pub width: u16,
    pub height: u16,
    pub image: ImageOutputEnum,
}

pub enum ImageOutputEnum {
    //Bitmap(*mut BltPixel),
    //Screen(*mut OutputProtocol),
}

#[repr(C)]
#[unsafe_protocol("9e66f251-727c-418c-bfd6-c2b4252818ea")]
pub struct HiiImageDecoder {
    get_image_decoder_name: extern "efiapi" fn(
        this: &Self,
        decoder_name: &mut *mut Guid,
        number_of_decoder_name: *mut u16,
    ) -> Status,
    get_image_info: extern "efiapi" fn(
        this: &Self,
        image: *const c_void,
        size_of_image: usize,
        *mut *mut ImageInfo,
    ) -> Status,
    decode_image: extern "efiapi" fn(
        this: &Self,
        image: *const c_void,
        size: usize,
        bitmap: &mut *mut ImageOutput,
        transparent: bool,
    ) -> Status,
}
impl HiiImageDecoder {
    pub fn get_image_decoder_names(&self) -> [DecoderName; 3] {
        //let mut decoder_names = [];
        let mut decoder_names = ptr::null_mut();
        let mut decoder_num = 0;
        let names = [DecoderName::Empty; 3];
        _ = (self.get_image_decoder_name)(self, &mut decoder_names, &mut decoder_num);
        names
    }
    pub fn get_image_info(&self) -> ImageInfo {
        //(self.get_image_info)(self);
        //let _header: ImageInfoHeader = unimplemented!();
        //match DecoderName::from(header.decoder_name) {
        //    DecoderName::Jpeg => unimplemented!(), // ImageInfo::Jpeg {}
        //    DecoderName::Png => unimplemented!(),  // ImageInfo::Png {}
        //    DecoderName::Other(_) => unimplemented!(), //ImageInfo::Other {}
        //    DecoderName::Empty => unreachable!(),
        //}
        unimplemented!()
    }
    /// Caller needs to deallocate returned image
    pub fn decode_image(&self, image: &[u8]) {
        let image_buffer = image.as_ptr() as *const c_void;
        let mut output_buffer = ptr::null_mut();

        _ = (self.decode_image)(self, image_buffer, image.len(), &mut output_buffer, false);

        // TODO: Wrap and return output_buffer pointer
        unimplemented!()
    }
}
