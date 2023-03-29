#![no_main]
#![no_std]
#![allow(clippy::result_unit_err)]

extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;

use log::info;
use uefi::table::boot::{OpenProtocolAttributes, OpenProtocolParams, SearchType};
use uefi::{prelude::*, CString16};
use uefi::{Identify, Result};

use crate::shell_params::*;
use crate::uefi_usb::*;
use crate::via::*;

pub mod raw_hid;
pub mod shell_params;
pub mod uefi_usb;
pub mod via;

const TIMEOUT: u32 = 3000;
const VERBOSE: bool = false;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Color {
    /// 0° and 255° => 0
    Red,
    /// Yellow (120°) => 43
    Yellow,
    /// Green (120°) => 85
    Green,
    /// Cyan (180°) => 125
    Cyan,
    /// Blue (240°) => 170
    Blue,
    /// Purple (300°) => 213
    Purple,
    /// Saturation 0
    White,
}
pub fn color_as_hue(color: Color) -> u8 {
    match color {
        Color::Red => 0,
        Color::Yellow => 43,
        Color::Green => 85,
        Color::Cyan => 125,
        Color::Blue => 170,
        Color::Purple => 213,
        Color::White => 0, // Doesn't matter, only hue needs to be 0
    }
}

#[entry]
fn main(image_handle: Handle, mut system_table: SystemTable<Boot>) -> Status {
    uefi_services::init(&mut system_table).unwrap();
    let boot_services = system_table.boot_services();

    let shell_params_h = boot_services
        .locate_handle_buffer(SearchType::ByProtocol(&ShellParameters::GUID))
        .unwrap();
    //info!("Found {} ShellParams handles", (*shell_params_h).len());
    for handle in &*shell_params_h {
        let params_handle = unsafe {
            boot_services
                .open_protocol::<ShellParameters>(
                    OpenProtocolParams {
                        handle: *handle,
                        agent: boot_services.image_handle(),
                        controller: None,
                    },
                    OpenProtocolAttributes::GetProtocol,
                )
                .expect("Failed to open UsbIo handle")
        };

        // Ehm why are there two and one has no args?
        if params_handle.argc == 0 {
            continue;
        }

        let args = params_handle.get_args();
        //info!("Args: {}", params_handle.argc);
        //info!("Args: {:?}", params_handle.get_args());

        if args.len() == 1 {
            info!("Use args:");
            info!(" --lsusb");
            info!(" --verbose");
            info!(" --bootloader");
            info!(" --brightness-low");
            info!(" --brightness-mid");
            info!(" --brightness-high");
            break;
        }
        for (i, _) in args.iter().enumerate() {
            if i == 0 {
                continue;
            }
            get_usb(boot_services, args).unwrap();
            break;
        }
    }

    //boot_services.stall(10_000_000);
    Status::SUCCESS
}

fn get_usb(bs: &BootServices, args: Vec<CString16>) -> Result {
    let usb_io_handles = bs.locate_handle_buffer(SearchType::ByProtocol(&UsbIo::GUID));

    let usb_io_handles = if let Ok(usb_io_handles) = usb_io_handles {
        usb_io_handles
    } else {
        info!("UsbIo protocol is missing");
        return Ok(());
    };

    let mut lsusb = false;
    let mut verbose = false;
    for arg in args.clone() {
        if arg.as_ref() == cstr16!("--lsusb") {
            lsusb = true;
        }
        if arg.as_ref() == cstr16!("--verbose") {
            verbose = true;
        }
    }

    if verbose {
        info!("Found {} UsbIo handles", (*usb_io_handles).len());
    }
    for handle in &*usb_io_handles {
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

        // Currently only interested in Keychron/Framework/Logitech keyboards
        // To avoid spamming the output
        if ![0x3434, 0x32AC, 0x046D].contains(&{ desc.id_vendor }) {
            continue;
        }
        if lsusb {
            info!("{:04X}:{:04X}", { desc.id_vendor }, { desc.id_product });
        }

        let major = (desc.bcd_device & 0xFF00) >> 8;
        let minor = (desc.bcd_device & 0xF0) >> 4;
        let patch = desc.bcd_device & 0x0F;
        if lsusb {
            info!(
                "  bcdDevice:       0x{:04X} ({:>2}.{:02}.{:02})",
                { desc.bcd_device },
                major,
                minor,
                patch
            );
            // What's this?
            //info!("  Protocol:        0x{:02X}", { desc.device_protocol });
            // 0 and 0 means look at the interface descriptor
            info!(
                "  Device Class:    {}, Subclass: {}",
                { desc.device_class },
                { desc.device_sub_class }
            );
            // TODO: Get string
            //info!("  Str Manufactr:   {}", { desc.str_manufacturer });
            //info!("  Str Product:     {}", { desc.str_product });
            // Is expected to be 1
            info!("  Configurations:  {}", { desc.num_configurations });
            // Is expected to be always be 64
            //info!("  Max Packet Size: {}", { desc.max_packet_size0 });
        }

        let interface = usb_handle
            .get_interface_descriptor()
            .expect("Failed to get Interface Descriptor");

        if lsusb {
            info!("  Interface #{}", { interface.interface_number });
            info!("    Endpoints:          {}", { interface.num_endpoints });
            info!("    Interface:          {}", { interface.interface });
            info!(
                "    Interface Class:    {}, Subclass: {}",
                { interface.interface_class },
                { interface.interface_sub_class }
            );
            //info!("    Interface Protocol: {}", {
            //    interface.interface_protocol
            //});
        }

        let config_desc = usb_handle
            .get_config_descriptor()
            .expect("Failed to get Config Descriptor");
        //info!("  Config Desc Total Len: {}", { config_desc.total_length });

        if interface.interface_class != 3 {
            info!("Skipping interface, not HID class");
            continue;
        }

        let interrupt_endpoint = find_interrupt_endpoint(&mut usb_handle, interface.num_endpoints);

        let mut all_descriptors = vec![0; config_desc.total_length as usize];
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
        //info!("Successfully got all descriptors!!!!");

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

        if lsusb {
            info!("  Found HID descriptor");
            info!("    Desc Type: {}", {
                hid_desc.hid_class_desc[0].descriptor_type
            });
        }

        let mut report_desc = Vec::new();
        report_desc.resize(hid_desc.hid_class_desc[0].descriptor_length as usize, 0);
        let mut request = UsbDeviceRequest {
            request_type: UsbRequestType::GetDescriptor as u8,
            request: DeviceRequest::UsbGetDescriptor as u8,
            value: u16::from_le_bytes([0, UsbDescriptorType::Report as u8]),
            index: interface.interface_number as u16,
            length: hid_desc.hid_class_desc[0].descriptor_length,
        };
        let status = usb_handle.control_transfer(
            &mut request,
            UsbDataDirection::DataIn,
            TIMEOUT,
            &mut report_desc,
        );
        //.expect("Failed to get report descriptor");
        if status.is_err() {
            info!("  Failed to get HID report descriptor! ({:?}", status);
            continue;
        }
        if verbose {
            info!("  Got HID report descriptor!");
        }

        let (in_endpoint_addr, out_endpoint_addr) =
            if let Some((input, output)) = interrupt_endpoint {
                assert!(input.is_in() && input.is_interrupt());
                assert!(!output.is_in() && output.is_interrupt());
                (input.endpoint_address, output.endpoint_address)
            } else {
                info!("Failed to find OUT Interrupt descriptor");
                continue;
            };

        let usage_page = parse_hid_descriptor(report_desc.as_slice());
        let _generic = UsagePage::GenericDesktop as u8;
        let _keyboard = UsagePage::Keyboard as u8;
        let _qmk_console = UsagePage::QmkConsole as u16;
        let _raw = UsagePage::Raw as u16;
        match usage_page {
            //Some(HidData::Uint8(generic)) => {}
            //Some(HidData::Uint8(keyboard)) => {}
            //Some(HidData::Uint16(qmk_console)) => {}
            Some(HidData::Uint16(0xFF60)) => {
                if verbose {
                    info!(
                        "  Found raw usage page at interface {}",
                        interface.interface_number
                    );
                }
                let mut dev = HidDevice {
                    usb_io: usb_handle,
                    interface_no: interface.interface_number,
                    in_endpoint_addr,
                    out_endpoint_addr,
                };

                let protocol_ver = get_protocol_ver(&mut dev);
                info!("Protocol version: {:?}", protocol_ver);
                for (i, arg) in args.iter().enumerate() {
                    if arg.as_ref() == cstr16!("--bootloader") {
                        info!("Jumping to bootloader");
                        bootloader_jump(&mut dev).unwrap();
                    } else if arg.as_ref() == cstr16!("--brightness-low") {
                        if i + 1 >= args.len() {
                            let percentage = 0;
                            let brightness = (255 * percentage) / 100;
                            set_rgb_u8(
                                &mut dev,
                                ViaRgbMatrixValue::Brightness as u8,
                                brightness as u8,
                            )
                            .unwrap();
                        }

                        let brightness =
                            get_rgb_u8(&mut dev, ViaRgbMatrixValue::Brightness as u8).unwrap();
                        let percentage = (100 * brightness as usize) / 255;
                        info!("Brightness: {}%", percentage);
                    } else if arg.as_ref() == cstr16!("--brightness-mid") {
                        if i + 1 >= args.len() {
                            let percentage = 50;
                            let brightness = (255 * percentage) / 100;
                            set_rgb_u8(
                                &mut dev,
                                ViaRgbMatrixValue::Brightness as u8,
                                brightness as u8,
                            )
                            .unwrap();
                        }

                        let brightness =
                            get_rgb_u8(&mut dev, ViaRgbMatrixValue::Brightness as u8).unwrap();
                        let percentage = (100 * brightness as usize) / 255;
                        info!("Brightness: {}%", percentage);
                    } else if arg.as_ref() == cstr16!("--brightness-high") {
                        if i + 1 >= args.len() {
                            let percentage = 100;
                            let brightness = (255 * percentage) / 100;
                            set_rgb_u8(
                                &mut dev,
                                ViaRgbMatrixValue::Brightness as u8,
                                brightness as u8,
                            )
                            .unwrap();
                        }

                        let brightness =
                            get_rgb_u8(&mut dev, ViaRgbMatrixValue::Brightness as u8).unwrap();
                        let percentage = (100 * brightness as usize) / 255;
                        info!("Brightness: {}%", percentage);
                    }
                }
                // Don't care about other pages
                break;
            }
            Some(HidData::Uint16(page)) => {
                info!(
                    "  Found usage page {page:04X} at interface {}",
                    interface.interface_number
                );
            }
            Some(data) => {
                info!("  Found usage page: {:?}", data);
            }
            None => {
                info!("  No usage page found??");
            }
        }
    }
    Ok(())
}
