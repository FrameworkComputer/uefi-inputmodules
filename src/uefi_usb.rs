use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::mem::MaybeUninit;
#[allow(unused_imports)]
use log::{debug, error, info, trace};
use uefi::prelude::*;
use uefi::proto::unsafe_protocol;
use uefi::table::boot::{OpenProtocolAttributes, OpenProtocolParams, SearchType};
use uefi::{Char16, Identify, Result};
#[allow(unused_imports)]
use uefi_services::{print, println};

// This file is a simple UsbIo protocol
// It's not supposed to be complete - for now we only need to read the device descriptor to get
// VID, PID and bcdUSB.

#[allow(dead_code)]
#[repr(C)]
pub enum UsbDataDirection {
    DataIn = 0x00,
    DataOut = 0x01,
    NoData = 0x02,
}

#[repr(C, packed)]
pub struct UsbDeviceRequest {
    pub request_type: u8,
    pub request: u8,
    pub value: u16,
    pub index: u16,
    pub length: u16,
}

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
        extern "efiapi" fn(&mut UsbIo, u8, *mut u8, *mut usize, usize, *mut u32) -> Status,
    usb_isochronous_transfer:
        extern "efiapi" fn(&mut UsbIo, u8, *mut u8, usize, *mut u32) -> Status,
    // TODO: async_isochronous_transfer
    usb_async_isochronous_transfer: extern "efiapi" fn(&mut UsbIo) -> Status,
    usb_get_device_descriptor: extern "efiapi" fn(&mut UsbIo, *mut UsbDeviceDescriptor) -> Status,
    usb_get_config_descriptor: extern "efiapi" fn(&mut UsbIo, *mut UsbConfigDescriptor) -> Status,
    usb_get_interface_descriptor:
        extern "efiapi" fn(&mut UsbIo, *mut UsbInterfaceDescriptor) -> Status,
    usb_get_endpoint_descriptor:
        extern "efiapi" fn(&mut UsbIo, u8, *mut UsbEndpointDescriptor) -> Status,
    usb_get_string_descriptor: extern "efiapi" fn(&mut UsbIo, u16, u8, *mut *mut Char16) -> Status,
    usb_get_supported_languages: extern "efiapi" fn(&mut UsbIo, *mut *mut u16, *mut u16) -> Status,
    usb_port_reset: extern "efiapi" fn(&mut UsbIo) -> Status,
}
impl UsbIo {
    pub fn get_device_descriptor(&mut self) -> Result<UsbDeviceDescriptor> {
        let mut desc = MaybeUninit::<UsbDeviceDescriptor>::uninit();
        (self.usb_get_device_descriptor)(self, desc.as_mut_ptr())
            .into_with_val(|| unsafe { desc.assume_init() })
    }
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub struct UsbDevice {
    pub vid: u16,
    pub pid: u16,
    pub bcd_device: u16,
}

impl UsbDevice {
    pub fn get_name(&self) -> Option<&str> {
        match (self.vid, self.pid) {
            (0x32AC, 0x0012) => Some("Framework 16 - ANSI Keyboard    "),
            (0x32AC, 0x0018) => Some("Framework 16 - ISO Keyboard     "),
            (0x32AC, 0x0019) => Some("Framework 16 - JIS Keyboard     "),
            (0x32AC, 0x0013) => Some("Framework 16 - Macropad         "),
            (0x32AC, 0x0014) => Some("Framework 16 - Numpad           "),
            (0x32AC, 0x0020) => Some("Framework 16 - LED Matrix       "),
            (0x32AC, 0x0030) => Some("Framework 16 - Copiilot Keyboard"),
            _ => None,
        }
    }
    /// If the VID is 0x32AC (Framework Computer)
    pub fn is_framework(&self) -> bool {
        self.vid == 0x32AC
    }
    pub fn is_framework_keyboard(&self) -> bool {
        self.is_framework() && [0x0012, 0x0018, 0x0019, 0x0013, 0x0014, 0x0030].contains(&self.pid)
    }

    pub fn format_bcd_device(&self) -> String {
        let major = (self.bcd_device & 0xFF00) >> 8;
        let minor = (self.bcd_device & 0xF0) >> 4;
        let patch = self.bcd_device & 0x0F;

        format!("{:>2}.{:02}.{:02}", major, minor, patch)
    }
}

pub fn get_usb_devices(bs: &BootServices) -> Result<Vec<UsbDevice>> {
    let mut devices = vec![];

    #[cfg(not(feature = "qemu"))]
    debug!("Locate USB IO Handles");
    let usb_io_handles = bs.locate_handle_buffer(SearchType::ByProtocol(&UsbIo::GUID));

    let usb_io_handles = if let Ok(usb_io_handles) = usb_io_handles {
        debug!("UsbIo protocol found");
        usb_io_handles
    } else {
        // In QEMU just return an emtpy list, it seems default OVMF does not have this protocol
        #[cfg(not(feature = "qemu"))]
        error!("UsbIo protocol is missing");

        return Ok(devices);
    };

    info!("Found {} UsbIo handles", (*usb_io_handles).len());
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

        let dev = UsbDevice {
            vid: desc.id_vendor,
            pid: desc.id_product,
            bcd_device: desc.bcd_device,
        };
        // Don't push identical devices - UsbIo protocol returns multiple instances for each device
        // I think one for each interface
        if !devices.contains(&dev) {
            devices.push(dev);
        }

        // Only interested in (Keychron/Framework/Logitech) keyboards
        // Listing 3rdparty vendor's keyboards to test on other Framework systems
        // To avoid spamming the output
        if ![0x3434, 0x32AC, 0x046D].contains(&{ desc.id_vendor }) {
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
        // What's this?
        debug!("  Protocol:        0x{:02X}", { desc.device_protocol });
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
    Ok(devices)
}

/// Same as get_usb_devices but return empty Vec instead of Result::Err
/// It prints the error message with error!
pub fn get_usb_devices_noerr(bs: &BootServices) -> Vec<UsbDevice> {
    match get_usb_devices(bs) {
        Ok(devices) => devices,
        Err(err) => {
            error!("{:?}", err);
            vec![]
        }
    }
}
