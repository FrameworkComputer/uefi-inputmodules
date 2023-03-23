#![no_main]
#![no_std]

use log::info;
use uefi::prelude::*;
use uefi::proto::unsafe_protocol;
use uefi::proto::device_path::text::{
    AllowShortcuts, DevicePathToText, DisplayOnly,
};
use uefi::proto::loaded_image::LoadedImage;
use uefi::table::boot::SearchType;
use uefi::{Identify, Result};
//use ptr_meta::Pointee;
use core::ptr;

#[repr(C)]
pub enum UsbDataDirection {
  DataIn,
  DataOut,
  NoData
}

//#[derive(Eq, Pointee)]
#[repr(C, packed)]
pub struct UsbDeviceRequest {
  pub request_type: u8,
  pub request: u8,
  pub value: u16,
  pub index: u16,
  pub length: u16,
}

//#[derive(Eq, Pointee)]
#[repr(C, packed)]
pub struct UsbDeviceDescriptor {
  pub length: u8,
  pub descriptor_type: u8,
  pub bcd_usb: u16,
  pub device_class: u8,
  pub device_sub_class: u8,
  pub device_protocol: u8,
  pub max_packet_size0: u8,
  pub id_vendor: u16,
  pub id_product: u16,
  pub bcd_device: u16,
  pub str_manufacturer: u8,
  pub str_product: u8,
  pub str_serial_number: u8,
  pub num_configurations: u8,
}


/// Foo
#[repr(C)]
#[unsafe_protocol("2b2f68d6-0cd2-44cf-8e8b-bba20b1b5b75")]
pub struct UsbIo {
    /// This one
    ///   IN EFI_USB_IO_PROTOCOL                        *This,
    //IN EFI_USB_DEVICE_REQUEST                     *Request,
    //IN EFI_USB_DATA_DIRECTION                     Direction,
    //IN UINT32                                     Timeout,
    //IN OUT VOID                                   *Data OPTIONAL,
    //IN UINTN                                      DataLength  OPTIONAL,

    usb_control_transfer: extern "efiapi" fn(&mut UsbIo, &mut UsbDeviceRequest, &mut UsbDataDirection, u32, *mut u8, &mut usize) -> Status,

    usb_bulk_transfer: extern "efiapi" fn(&mut UsbIo) -> Status,
    usb_async_interrupt_transfer: extern "efiapi" fn(&mut UsbIo) -> Status,
    usb_sync_interrupt_transfer: extern "efiapi" fn(&mut UsbIo) -> Status,
    usb_isochronous_transfer: extern "efiapi" fn(&mut UsbIo) -> Status,
    usb_async_isochronous_transfer: extern "efiapi" fn(&mut UsbIo) -> Status,

    /// This one
    ///   OUT EFI_USB_DEVICE_DESCRIPTOR     *DeviceDescriptor
    usb_get_device_descriptor: extern "efiapi" fn(&mut UsbIo, *mut UsbDeviceDescriptor) -> Status,

    usb_get_config_descriptor: extern "efiapi" fn(&mut UsbIo) -> Status,
    usb_get_interface_descriptor: extern "efiapi" fn(&mut UsbIo) -> Status,
    usb_get_endpoint_descriptor: extern "efiapi" fn(&mut UsbIo) -> Status,
    usb_get_string_descriptor: extern "efiapi" fn(&mut UsbIo) -> Status,
    usb_get_supported_languages: extern "efiapi" fn(&mut UsbIo) -> Status,
    usb_port_reset: extern "efiapi" fn(&mut UsbIo) -> Status,
}
impl UsbIo {
    /// Control transfer
    pub fn control_transfer(
        &mut self,
        request: &mut UsbDeviceRequest,
        direction: &mut UsbDataDirection,
        timeout: u32,
        data: &mut [u8]
    ) -> Result<(), usize> {
        let mut buffer_size = data.len();
        unsafe {
            (self.usb_control_transfer)(
                self,
                request,
                direction,
                timeout,
                data.as_mut_ptr(),
                &mut buffer_size,
            )
        }.into_with(
            || debug_assert_eq!(buffer_size, data.len()),
            |_| buffer_size,
        )
    }

    pub fn get_device_descriptor(&mut self) -> Result<&UsbDeviceDescriptor> {
        let ptr: *mut UsbDeviceDescriptor = ptr::null_mut();
        (self.usb_get_device_descriptor)(self, ptr).into_with_val(|| {
            unsafe { ptr.as_ref().unwrap() }
        })
    }
}

#[entry]
fn main(image_handle: Handle, mut system_table: SystemTable<Boot>) -> Status {
    uefi_services::init(&mut system_table).unwrap();
    let boot_services = system_table.boot_services();

    get_usb(boot_services).unwrap();
    print_image_path(boot_services).unwrap();

    boot_services.stall(10_000_000);
    Status::SUCCESS
}

fn get_usb(bs: &BootServices) -> Result {
    let usb_io_handles = bs
        .locate_handle_buffer(SearchType::ByProtocol(&UsbIo::GUID))
        .expect("UsbIo is missing");

    info!("Found {} UsbIo handles", (*usb_io_handles).len());
    for handle in &*usb_io_handles {
        info!("Iterating through UsbIo handles");
    }
    Ok(())
}

fn print_image_path(boot_services: &BootServices) -> Result {
    let loaded_image = boot_services
        .open_protocol_exclusive::<LoadedImage>(boot_services.image_handle())?;

    let device_path_to_text_handle = *boot_services
        .locate_handle_buffer(SearchType::ByProtocol(&DevicePathToText::GUID))?
        .first()
        .expect("DevicePathToText is missing");

    let device_path_to_text = boot_services
        .open_protocol_exclusive::<DevicePathToText>(
            device_path_to_text_handle,
        )?;

    let image_device_path =
        loaded_image.file_path().expect("File path is not set");
    let image_device_path_text = device_path_to_text
        .convert_device_path_to_text(
            boot_services,
            image_device_path,
            DisplayOnly(true),
            AllowShortcuts(false),
        )
        .expect("convert_device_path_to_text failed");

    info!("Image path: {}", &*image_device_path_text);
    Ok(())
}
