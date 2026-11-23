use alloc::vec;
use alloc::vec::Vec;
#[allow(unused_imports)]
use log::{debug, error, info, trace};
use uefi::boot::{self, OpenProtocolAttributes, OpenProtocolParams, SearchType};
use uefi::proto::usb::io::UsbIo;
use uefi::{Identify, Result};

use crate::usb::UsbDevice;

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
