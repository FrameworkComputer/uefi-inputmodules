//! Interact with the raw HID interface of QMK firmware

use crate::HidDevice;
use alloc::vec;
use alloc::vec::Vec;
use log::info;

// ChibiOS won't respond if we send 32.
// Currently I hardcoded it to ignore. But in upstream QMK/ChibiOS we have to send 33 bytes.
// See raw_hid_send in tmk_core/protocol/chibios/usb_main.c
pub const RAW_HID_BUFFER_SIZE: usize = 32;

pub fn send_message(
    dev: &mut HidDevice,
    message_id: u8,
    msg: Option<&[u8]>,
    out_len: usize,
) -> Result<Vec<u8>, ()> {
    // TODO: Why fill the rest with 0xFE? hidapitester uses 0x00
    let mut data = vec![0xFE; RAW_HID_BUFFER_SIZE];
    data[0] = 0x00; // NULL report ID
    data[1] = message_id;

    if let Some(msg) = msg {
        assert!(msg.len() <= RAW_HID_BUFFER_SIZE);
        let data_msg = &mut data[2..msg.len() + 2];
        data_msg.copy_from_slice(msg);
    }

    info!("Writing data: {:?}", data);
    let res = dev.write(&mut data);
    match res {
        Ok(size) => {
            info!("Written: {}", size);
        }
        Err(err) => {
            info!("Write err: {err:?}");
            return Err(());
        }
    };

    // No response expected
    if out_len == 0 {
        return Ok(vec![]);
    }

    dev.set_blocking_mode(true).unwrap();

    let mut buf: Vec<u8> = vec![0xFE; RAW_HID_BUFFER_SIZE];
    let res = dev.read(buf.as_mut_slice());
    match res {
        Ok(size) => {
            info!("Read: {}", size);
            info!("out_len: {}", out_len);
            info!("buf: {:?}", buf);
            Ok(buf[1..out_len + 1].to_vec())
        }
        Err(err) => {
            info!("Read err: {err:?}");
            Err(())
        }
    }
}
