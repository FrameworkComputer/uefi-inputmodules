use embedded_graphics::prelude::Point;

#[allow(unused_imports)]
use log::{debug, error, info, trace};
use uefi::prelude::{BootServices, RuntimeServices};
use uefi::proto::console::gop::GraphicsOutput;
use uefi::proto::console::text::Input;
use uefi::table::boot::{OpenProtocolAttributes, OpenProtocolParams, ScopedProtocol, SearchType};
use uefi::table::runtime::{VariableAttributes, VariableVendor};
use uefi::{cstr16, guid, Identify};
#[allow(unused_imports)]
use uefi_services::{print, println, system_table};

use crate::buffer::Buffer;

const EFI_OS_INDICATIONS_BOOT_TO_FW_UI: u64 = 0x0000000000000001;
const EFI_GLOBAL_VARIABLE_GUID: uefi::Guid = guid!("8be4df61-93ca-11d2-aa0d-00e098032b8c");

pub fn get_gop_buffer(bs: &BootServices) -> Option<(ScopedProtocol<GraphicsOutput>, Buffer)> {
    let gop_handles = if let Ok(gop_handles) =
        bs.locate_handle_buffer(SearchType::ByProtocol(&GraphicsOutput::GUID))
    {
        gop_handles
    } else {
        info!("GraphicsOutput protocol is missing");
        return None;
    };

    let mut gop = if let Some(handle) = gop_handles.iter().next() {
        unsafe {
            bs.open_protocol::<GraphicsOutput>(
                OpenProtocolParams {
                    handle: *handle,
                    agent: bs.image_handle(),
                    controller: None,
                },
                OpenProtocolAttributes::GetProtocol,
            )
            .expect("Failed to open GraphicsOutput handle")
        }
    } else {
        info!("No GOP handles found");
        return None;
    };

    // Switch to the highest available resolution (largest pixel count).
    if let Some(highest) = gop.modes().max_by_key(|mode| {
        let (width, height) = mode.info().resolution();
        width * height
    }) {
        let (width, height) = highest.info().resolution();
        info!("Switching to highest resolution: {}x{}", width, height);
        if let Err(err) = gop.set_mode(&highest) {
            error!("Failed to set GOP mode: {:?}", err);
        }
    }

    let mode = gop.current_mode_info();
    let (width, height) = mode.resolution();
    let _size = Point::new(width as i32, height as i32);
    info!("Current Resolution {}x{}", width, height);

    // Create a buffer to draw into.
    let (width, height) = gop.current_mode_info().resolution();
    Some((gop, Buffer::new(width, height)))
}

pub fn get_input_protocol(bs: &BootServices) -> Option<ScopedProtocol<Input>> {
    let input_handles =
        if let Ok(input_handles) = bs.locate_handle_buffer(SearchType::ByProtocol(&Input::GUID)) {
            input_handles
        } else {
            info!("Input protocol is missing");
            return None;
        };

    if let Some(handle) = input_handles.iter().next() {
        unsafe {
            bs.open_protocol::<Input>(
                OpenProtocolParams {
                    handle: *handle,
                    agent: bs.image_handle(),
                    controller: None,
                },
                OpenProtocolAttributes::GetProtocol,
            )
            .ok()
        }
    } else {
        info!("No Input handles found");
        None
    }
}

// Avoid using because it might cause trouble with later boot process
fn _get_exclusive_input_protocol(bs: &BootServices) -> Option<ScopedProtocol<Input>> {
    let input_handle = bs.get_handle_for_protocol::<Input>().ok()?;
    bs.open_protocol_exclusive::<Input>(input_handle).ok()
}

/// Set OS indications so that a reboot will boot into setup menu
pub fn set_os_indications_setupmenu(rt: &RuntimeServices) {
    let vendor = VariableVendor(EFI_GLOBAL_VARIABLE_GUID);
    let mut buf = [0; 8];
    rt.get_variable(cstr16!("OsIndicationsSupported"), &vendor, &mut buf)
        .expect("failed to get variable");
    let os_indications: u64 = u64::from_ne_bytes(buf);
    debug!("OS Indications: {:?}", os_indications);

    let os_indications = if rt
        .get_variable(cstr16!("OsIndications"), &vendor, &mut buf)
        .is_ok()
    {
        u64::from_ne_bytes(buf) | EFI_OS_INDICATIONS_BOOT_TO_FW_UI
    } else {
        EFI_OS_INDICATIONS_BOOT_TO_FW_UI
    };
    debug!("OS Indications: {:?}", os_indications);

    let attrs = VariableAttributes::NON_VOLATILE
        | VariableAttributes::BOOTSERVICE_ACCESS
        | VariableAttributes::RUNTIME_ACCESS;
    rt.set_variable(
        cstr16!("OsIndications"),
        &vendor,
        attrs,
        &u64::to_ne_bytes(os_indications),
    )
    .expect("failed to set variable");
}
