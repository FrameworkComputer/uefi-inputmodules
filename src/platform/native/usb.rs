use nusb::MaybeFuture;

use crate::usb::UsbDevice;

pub fn get_usb_devices() -> Result<Vec<UsbDevice>, nusb::Error> {
    let mut devices = vec![];
    for info in nusb::list_devices().wait()? {
        let dev = UsbDevice {
            vid: info.vendor_id(),
            pid: info.product_id(),
            bcd_device: info.device_version(),
        };
        if !devices.contains(&dev) {
            devices.push(dev);
        }
    }
    Ok(devices)
}
