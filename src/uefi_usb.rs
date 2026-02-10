use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
#[allow(unused_imports)]
use log::{debug, error, info, trace};
use uefi::boot::{self, OpenProtocolAttributes, OpenProtocolParams, SearchType};
use uefi::proto::usb::io::UsbIo;
use uefi::{Identify, Result};

#[derive(PartialEq, Eq, Debug, Clone)]
pub struct UsbDevice {
    pub vid: u16,
    pub pid: u16,
    pub bcd_device: u16,
}

impl UsbDevice {
    pub fn get_name(&self) -> Option<&str> {
        match (self.vid, self.pid) {
            (0x32AC, 0x0012) => Some("Framework Laptop 16 - ANSI Keyboard   "),
            (0x32AC, 0x0018) => Some("Framework Laptop 16 - ISO Keyboard    "),
            (0x32AC, 0x0019) => Some("Framework Laptop 16 - JIS Keyboard    "),
            (0x32AC, 0x0013) => Some("Framework Laptop 16 - Macropad        "),
            (0x32AC, 0x0014) => Some("Framework Laptop 16 - Numpad          "),
            (0x32AC, 0x0020) => Some("Framework Laptop 16 - LED Matrix      "),
            (0x32AC, 0x0030) => Some("Framework Laptop 16 - Copilot Keyboard"),
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

pub fn get_usb_devices() -> Result<Vec<UsbDevice>> {
    let mut devices = vec![];

    #[cfg(not(feature = "qemu"))]
    debug!("Locate USB IO Handles");
    let usb_io_handles = boot::locate_handle_buffer(SearchType::ByProtocol(&UsbIo::GUID));

    let usb_io_handles = if let Ok(usb_io_handles) = usb_io_handles {
        debug!("UsbIo protocol found");
        usb_io_handles
    } else {
        // In QEMU just return an emtpy list, it seems default OVMF does not have this protocol
        #[cfg(not(feature = "qemu"))]
        error!("UsbIo protocol is missing");

        return Ok(devices);
    };

    info!("Found {} UsbIo handles", usb_io_handles.len());
    for handle in usb_io_handles.iter() {
        let mut usb_handle = unsafe {
            boot::open_protocol::<UsbIo>(
                OpenProtocolParams {
                    handle: *handle,
                    agent: boot::image_handle(),
                    controller: None,
                },
                OpenProtocolAttributes::GetProtocol,
            )
            .expect("Failed to open UsbIo handle")
        };

        let desc = usb_handle
            .device_descriptor()
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
        debug!("  Protocol:        0x{:02X}", desc.device_protocol);
        // 0 and 0 means look at the interface descriptor
        info!(
            "  Device Class:    {}, Subclass: {}",
            desc.device_class, desc.device_subclass
        );
        // TODO: Get string
        //info!("  Str Manufactr:   {}", { desc.str_manufacturer });
        //info!("  Str Product:     {}", { desc.str_product });
        // Is expected to be 1
        info!("  Configurations:  {}", desc.num_configurations);
        // Is expected to be always be 64
        //info!("  Max Packet Size: {}", { desc.max_packet_size0 });
    }
    Ok(devices)
}

/// Same as get_usb_devices but return empty Vec instead of Result::Err
/// It prints the error message with error!
pub fn get_usb_devices_noerr() -> Vec<UsbDevice> {
    match get_usb_devices() {
        Ok(devices) => devices,
        Err(err) => {
            error!("{:?}", err);
            vec![]
        }
    }
}
