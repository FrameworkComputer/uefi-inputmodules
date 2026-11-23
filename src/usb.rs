use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
#[allow(unused_imports)]
use log::{debug, error, info, trace};

pub use crate::platform::get_usb_devices;

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
