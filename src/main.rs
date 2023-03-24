#![no_main]
#![no_std]

extern crate alloc;

use alloc::vec::Vec;

use log::info;
use uefi::proto::device_path::text::{AllowShortcuts, DevicePathToText, DisplayOnly};
use uefi::proto::loaded_image::LoadedImage;
use uefi::proto::unsafe_protocol;
use uefi::table::boot::{OpenProtocolAttributes, OpenProtocolParams, ScopedProtocol, SearchType};
use uefi::{prelude::*, CStr16};
use uefi::{Char16, Identify, Result};
//use ptr_meta::Pointee;
use core::mem::MaybeUninit;

const TIMEOUT: u32 = 3000;

#[repr(C)]
pub enum UsbDataDirection {
    DataIn = 0x00,
    DataOut = 0x01,
    NoData = 0x02,
}

#[repr(u8)]
pub enum UsbDescriptorType {
    Device = 0x01,
    Config = 0x02,
    String = 0x03,
    Interface = 0x04,
    Endpoint = 0x05,
    Hid = 0x21,
    Report = 0x22,
}

#[repr(C, packed)]
pub struct HidDescriptor {
    pub length: u8,
    pub descriptor_type: u8,
    pub bcd_hid: u16,
    pub country_code: u8,
    pub num_descriptors: u8,
    pub hid_class_desc: [HidClassDescriptor; 1],
}

#[repr(C, packed)]
pub struct HidClassDescriptor {
    pub descriptor_type: u8,
    pub descriptor_length: u16,
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

#[repr(C, packed)]
pub struct UsbConfigDescriptor {
    pub length: u8,
    pub descriptor_type: u8,
    pub total_length: u16,
    pub num_interfaces: u8,
    pub configuration_value: u8,
    pub configuration: u8,
    pub attributes: u8,
    pub max_power: u8,
}

#[repr(C, packed)]
pub struct UsbInterfaceDescriptor {
    pub length: u8,
    pub descriptor_type: u8,
    pub interface_number: u8,
    pub alternate_setting: u8,
    pub num_endpoints: u8,
    pub interface_class: u8,
    pub interface_sub_class: u8,
    pub interface_protocol: u8,
    pub interface: u8,
}

#[repr(C, packed)]
pub struct UsbEndpointDescriptor {
    pub length: u8,
    pub descriptor_type: u8,
    pub endpoint_address: u8,
    pub attributes: u8,
    pub max_packet_size: u16,
    pub interval: u8,
}

#[repr(C, packed)]
pub struct UsbDescriptorHead {
    pub len: u8,
    pub desc_type: u8,
}

/// Foo
#[repr(C)]
#[unsafe_protocol("2b2f68d6-0cd2-44cf-8e8b-bba20b1b5b75")]
pub struct UsbIo {
    usb_control_transfer: extern "efiapi" fn(
        &mut UsbIo,
        &mut UsbDeviceRequest,
        UsbDataDirection,
        u32,
        *mut u8,
        usize,
        *mut u32,
    ) -> Status,

    usb_bulk_transfer:
        extern "efiapi" fn(&mut UsbIo, u8, *mut u8, *mut usize, usize, u32) -> Status,
    // TODO: usb_async_interrupt_transfer
    usb_async_interrupt_transfer: extern "efiapi" fn(&mut UsbIo) -> Status,
    usb_sync_interrupt_transfer:
        extern "efiapi" fn(&mut UsbIo, u8, *mut u8, *mut usize, usize, u32) -> Status,
    usb_isochronous_transfer:
        extern "efiapi" fn(&mut UsbIo, u8, *mut u8, usize, *mut u32) -> Status,
    // TODO: async_isochronous_transfer
    usb_async_isochronous_transfer: extern "efiapi" fn(&mut UsbIo) -> Status,
    usb_get_device_descriptor: extern "efiapi" fn(&mut UsbIo, *mut UsbDeviceDescriptor) -> Status,
    usb_get_config_descriptor: extern "efiapi" fn(&mut UsbIo, *mut UsbConfigDescriptor) -> Status,
    usb_get_interface_descriptor:
        extern "efiapi" fn(&mut UsbIo, *mut UsbInterfaceDescriptor) -> Status,
    usb_get_endpoint_descriptor:
        extern "efiapi" fn(&mut UsbIo, *mut UsbEndpointDescriptor) -> Status,
    usb_get_string_descriptor: extern "efiapi" fn(&mut UsbIo, u16, u8, *mut *mut Char16) -> Status,
    usb_get_supported_languages: extern "efiapi" fn(&mut UsbIo, *mut *mut u16, *mut u16) -> Status,
    usb_port_reset: extern "efiapi" fn(&mut UsbIo) -> Status,
}
impl UsbIo {
    pub fn control_transfer(
        &mut self,
        request: &mut UsbDeviceRequest,
        direction: UsbDataDirection,
        timeout: u32,
        data: &mut [u8],
    ) -> Result<(), usize> {
        let mut buffer_size = data.len();
        let mut status = 0; // TODO: Use this value?
        (self.usb_control_transfer)(
            self,
            request,
            direction,
            timeout,
            data.as_mut_ptr(),
            buffer_size,
            &mut status,
        )
        .into_with(
            || debug_assert_eq!(buffer_size, data.len()),
            |_| buffer_size,
        )
    }

    //pub fn get_string_descriptor(&mut self, lang_id: u16, string_id: u8) -> Result<CStr16> {
    //    (self.get_string_descriptor)(self, lang_id, string_id, )
    //}

    pub fn get_device_descriptor(&mut self) -> Result<UsbDeviceDescriptor> {
        let mut desc = MaybeUninit::<UsbDeviceDescriptor>::uninit();
        (self.usb_get_device_descriptor)(self, desc.as_mut_ptr())
            .into_with_val(|| unsafe { desc.assume_init() })
    }

    pub fn get_interface_descriptor(&mut self) -> Result<UsbInterfaceDescriptor> {
        let mut desc = MaybeUninit::<UsbInterfaceDescriptor>::uninit();
        (self.usb_get_interface_descriptor)(self, desc.as_mut_ptr())
            .into_with_val(|| unsafe { desc.assume_init() })
    }

    pub fn get_config_descriptor(&mut self) -> Result<UsbConfigDescriptor> {
        let mut desc = MaybeUninit::<UsbConfigDescriptor>::uninit();
        (self.usb_get_config_descriptor)(self, desc.as_mut_ptr())
            .into_with_val(|| unsafe { desc.assume_init() })
    }

    pub fn get_endpoint_descriptor(&mut self) -> Result<UsbEndpointDescriptor> {
        let mut desc = MaybeUninit::<UsbEndpointDescriptor>::uninit();
        (self.usb_get_endpoint_descriptor)(self, desc.as_mut_ptr())
            .into_with_val(|| unsafe { desc.assume_init() })
    }
}

#[entry]
fn main(image_handle: Handle, mut system_table: SystemTable<Boot>) -> Status {
    uefi_services::init(&mut system_table).unwrap();
    let boot_services = system_table.boot_services();

    get_usb(boot_services).unwrap();
    //print_image_path(boot_services).unwrap();

    //boot_services.stall(10_000_000);
    Status::SUCCESS
}

#[repr(u8)]
enum UsbRequestType {
    HidGetRequest = 0xA1,
    HidSetRequest = 0x21,
    GetDescriptor = 0x80,
}

enum HidRequest {
    GetReportRequest = 0x01,
    GetIdleRequest = 0x02,
    GetProtocolRequest = 0x03,
    SetReportRequest = 0x09,
    SetIdleRequest = 0x0a,
    SetProtocolRequest = 0x0b,
}

enum DeviceRequest {
    UsbGetDescriptor = 0x06,
    UsbSetDescriptor = 0x07,
}

fn set_report_request(
    usb_io: &mut UsbIo,
    interface: u8,
    report_id: u8,
    report_type: u8,
    report: &mut [u8],
) -> Result {
    let mut request = UsbDeviceRequest {
        request_type: UsbRequestType::HidSetRequest as u8,
        request: HidRequest::SetReportRequest as u8,
        value: u16::from_le_bytes([report_type, report_id]),
        index: interface as u16,
        length: report.len() as u16,
    };
    usb_io
        .control_transfer(&mut request, UsbDataDirection::DataOut, TIMEOUT, report)
        .unwrap();
    Ok(())
}

fn get_report_request(
    usb_io: &mut UsbIo,
    interface: u8,
    report_id: u8,
    report_type: u8,
    report: &mut [u8],
) -> Result {
    let mut request = UsbDeviceRequest {
        request_type: UsbRequestType::HidGetRequest as u8,
        request: HidRequest::SetReportRequest as u8,
        value: u16::from_le_bytes([report_type, report_id]),
        index: interface as u16,
        length: report.len() as u16,
    };
    usb_io
        .control_transfer(&mut request, UsbDataDirection::DataIn, TIMEOUT, report)
        .unwrap();
    Ok(())
}

fn get_usb(bs: &BootServices) -> Result {
    let usb_io_handles = bs
        .locate_handle_buffer(SearchType::ByProtocol(&UsbIo::GUID))
        .expect("UsbIo is missing");

    //info!("Found {} UsbIo handles", (*usb_io_handles).len());
    for handle in &*usb_io_handles {
        //info!("  Iterating through UsbIo handles");

        let mut usb_handle = unsafe {
            bs.open_protocol::<UsbIo>(
                OpenProtocolParams {
                    handle: *handle,
                    agent: bs.image_handle(),
                    controller: None,
                },
                OpenProtocolAttributes::GetProtocol,
            )
            .expect("Failed to open UsbIo handle")
        };

        let desc = usb_handle
            .get_device_descriptor()
            .expect("Failed to get Device Descriptor");

        // Currently only interested in Keychron/Framework keyboards
        if desc.id_vendor != 0x3434 && desc.id_vendor != 0x32AC {
            continue;
        }
        info!("{:04X}:{:04X}", { desc.id_vendor }, { desc.id_product });

        let major = (desc.bcd_device & 0xFF00) >> 8;
        let minor = (desc.bcd_device & 0xF0) >> 4;
        let patch = desc.bcd_device & 0x0F;
        info!(
            "  bcdDevice:       0x{:04X} ({:>2}.{:02}.{:02})",
            { desc.bcd_device },
            major,
            minor,
            patch
        );
        info!("  Protocol:        0x{:02X}", { desc.device_protocol });
        // 0 and 0 means look at the interface descriptor
        info!(
            "  Device Class:    {}, Subclass: {}",
            { desc.device_class },
            { desc.device_sub_class }
        );
        info!("  Num Configs:     {}", { desc.num_configurations });
        info!("  Str Manufactr:   {}", { desc.str_manufacturer });
        info!("  Str Product:     {}", { desc.str_product });
        info!("  Configurations:  {}", { desc.num_configurations });
        // Should always be 64
        //info!("  Max Packet Size: {}", { desc.max_packet_size0 });

        let interface = usb_handle
            .get_interface_descriptor()
            .expect("Failed to get Interface Descriptor");

        info!("  Interface #{}", { interface.interface_number });
        info!("    Endpoints:          {}", { interface.num_endpoints });
        info!("    Interface:          {}", { interface.interface });
        info!(
            "    Interface Class:    {}, Subclass: {}",
            { interface.interface_class },
            { interface.interface_sub_class }
        );
        info!("    Interface Protocol: {}", {
            interface.interface_protocol
        });

        let config_desc = usb_handle
            .get_config_descriptor()
            .expect("Failed to get Config Descriptor");
        info!("  Config Desc Total Len: {}", { config_desc.total_length });

        let mut all_descriptors = Vec::with_capacity(config_desc.total_length as usize);
        all_descriptors.resize(config_desc.total_length as usize, 0);
        let mut request = UsbDeviceRequest {
            request_type: UsbRequestType::GetDescriptor as u8,
            request: DeviceRequest::UsbGetDescriptor as u8,
            value: u16::from_le_bytes([
                config_desc.configuration_value - 1,
                UsbDescriptorType::Config as u8,
            ]),
            index: 0,
            length: config_desc.total_length,
        };
        usb_handle
            .control_transfer(
                &mut request,
                UsbDataDirection::DataIn,
                TIMEOUT,
                &mut all_descriptors,
            )
            .expect("Failed to get all descriptors :( :(");
        info!("Successfully got all descriptors!!!!");

        let mut all_desc_byte = 0;
        let mut after_interface_desc = false;
        let mut hid_desc: Option<&HidDescriptor> = None;
        while all_desc_byte < config_desc.total_length {
            let desc_head = unsafe {
                &*all_descriptors
                    .as_ptr()
                    .add(all_desc_byte as usize)
                    .cast::<UsbDescriptorHead>()
            };
            if desc_head.desc_type == (UsbDescriptorType::Interface as u8) {
                let found_int_desc = unsafe {
                    &*all_descriptors
                        .as_ptr()
                        .add(all_desc_byte as usize)
                        .cast::<UsbInterfaceDescriptor>()
                };
                if found_int_desc.interface_number == interface.interface_number
                    && found_int_desc.alternate_setting == interface.alternate_setting
                {
                    after_interface_desc = true;
                }
            }
            if after_interface_desc && desc_head.desc_type == (UsbDescriptorType::Endpoint as u8) {
                break;
            }
            if after_interface_desc && desc_head.desc_type == (UsbDescriptorType::Hid as u8) {
                hid_desc = Some(unsafe {
                    &*all_descriptors
                        .as_ptr()
                        .add(all_desc_byte as usize)
                        .cast::<HidDescriptor>()
                });
            }
            all_desc_byte += desc_head.len as u16;
        }
        let hid_desc = if let Some(hid_desc) = hid_desc {
            hid_desc
        } else {
            info!("  Failed to find HID descriptor");
            continue;
        };
        info!("  Found HID descriptor");
        info!("    Desc Type: {}", {
            hid_desc.hid_class_desc[0].descriptor_type
        });

        let mut report_desc = Vec::new();
        report_desc.resize(hid_desc.hid_class_desc[0].descriptor_length as usize, 0);
        let mut request = UsbDeviceRequest {
            request_type: UsbRequestType::GetDescriptor as u8,
            request: DeviceRequest::UsbGetDescriptor as u8,
            value: u16::from_le_bytes([0, UsbDescriptorType::Report as u8]),
            index: interface.interface_number as u16,
            length: hid_desc.hid_class_desc[0].descriptor_length,
        };
        usb_handle
            .control_transfer(
                &mut request,
                UsbDataDirection::DataIn,
                TIMEOUT,
                &mut report_desc,
            )
            .expect("Failed to get report descriptor");
        info!("  Got HID report descriptor!!!!");

        //if desc.id_vendor == 0x3434 || desc.id_vendor == 0x32AC {
        //    //set_key_led(&usb_handle, &desc, 0);
        //    //set_key_led(&usb_handle, &desc, 1);
        //    //set_key_led(&usb_handle, &desc, 2);
        //    //set_key_led(&usb_handle, &desc, 3);
        //}
    }
    Ok(())
}

fn set_key_led(usb_handle: &ScopedProtocol<UsbIo>, desc: &UsbDeviceDescriptor, interface: u16) {}

fn print_image_path(boot_services: &BootServices) -> Result {
    let loaded_image =
        boot_services.open_protocol_exclusive::<LoadedImage>(boot_services.image_handle())?;

    let device_path_to_text_handle = *boot_services
        .locate_handle_buffer(SearchType::ByProtocol(&DevicePathToText::GUID))?
        .first()
        .expect("DevicePathToText is missing");

    let device_path_to_text =
        boot_services.open_protocol_exclusive::<DevicePathToText>(device_path_to_text_handle)?;

    let image_device_path = loaded_image.file_path().expect("File path is not set");
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
