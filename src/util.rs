use embedded_graphics::prelude::Point;

#[allow(unused_imports)]
use log::{debug, error, info, trace};
use uefi::boot::{self, OpenProtocolAttributes, OpenProtocolParams, ScopedProtocol, SearchType};
use uefi::proto::console::gop::{GraphicsOutput, Mode};
use uefi::proto::console::text::Input;
use uefi::runtime::{VariableAttributes, VariableVendor};
use uefi::{cstr16, guid, Identify};
#[allow(unused_imports)]
use uefi::{print, println};

use crate::buffer::Buffer;

const EFI_OS_INDICATIONS_BOOT_TO_FW_UI: u64 = 0x0000000000000001;
const EFI_GLOBAL_VARIABLE_GUID: uefi::Guid = guid!("8be4df61-93ca-11d2-aa0d-00e098032b8c");

pub fn print_gop_info() -> Option<()> {
    let gop_handles = if let Ok(gop_handles) =
        boot::locate_handle_buffer(SearchType::ByProtocol(&GraphicsOutput::GUID))
    {
        gop_handles
    } else {
        error!("GraphicsOutput protocol is missing");
        return None;
    };

    let gop = if let Some(handle) = gop_handles.iter().next() {
        unsafe {
            boot::open_protocol::<GraphicsOutput>(
                OpenProtocolParams {
                    handle: *handle,
                    agent: boot::image_handle(),
                    controller: None,
                },
                OpenProtocolAttributes::GetProtocol,
            )
            .expect("Failed to open GraphicsOutput handle")
        }
    } else {
        error!("No GOP handles found");
        return None;
    };

    println!("Available resolutions:");
    for mode in gop.modes() {
        let (width, height) = mode.info().resolution();
        println!("  {}x{}", width, height);
    }

    let mode = gop.current_mode_info();
    let (width, height) = mode.resolution();
    println!("Current Resolution: {}x{}", width, height);

    Some(())
}

pub fn get_gop_buffer() -> Option<(ScopedProtocol<GraphicsOutput>, Buffer, Option<Mode>)> {
    let gop_handles = if let Ok(gop_handles) =
        boot::locate_handle_buffer(SearchType::ByProtocol(&GraphicsOutput::GUID))
    {
        gop_handles
    } else {
        info!("GraphicsOutput protocol is missing");
        return None;
    };

    let mut gop = if let Some(handle) = gop_handles.iter().next() {
        unsafe {
            boot::open_protocol::<GraphicsOutput>(
                OpenProtocolParams {
                    handle: *handle,
                    agent: boot::image_handle(),
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

    // Remember the original mode so the caller can restore it on exit.
    let original_info = gop.current_mode_info();
    let original_mode = gop.modes().find(|mode| *mode.info() == original_info);
    const NATIVE_RESOLUTION: (usize, usize) = (1920, 1200);

    if original_info.resolution() == NATIVE_RESOLUTION {
        // Already at the native resolution, nothing to do.
    } else if let Some(native) = gop
        .modes()
        .find(|mode| mode.info().resolution() == NATIVE_RESOLUTION)
    {
        // The native resolution is available, prefer it over anything else.
        let (width, height) = native.info().resolution();
        info!("Switching to native resolution: {}x{}", width, height);
        if let Err(err) = gop.set_mode(&native) {
            error!("Failed to set GOP mode: {:?}", err);
        }
    } else if let Some(highest) = gop.modes().max_by_key(|mode| {
        // Fall back to the highest available resolution (largest pixel count).
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
    Some((gop, Buffer::new(width, height), original_mode))
}

pub fn get_input_protocol() -> Option<ScopedProtocol<Input>> {
    let input_handles = if let Ok(input_handles) =
        boot::locate_handle_buffer(SearchType::ByProtocol(&Input::GUID))
    {
        input_handles
    } else {
        info!("Input protocol is missing");
        return None;
    };

    if let Some(handle) = input_handles.iter().next() {
        unsafe {
            boot::open_protocol::<Input>(
                OpenProtocolParams {
                    handle: *handle,
                    agent: boot::image_handle(),
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
fn _get_exclusive_input_protocol() -> Option<ScopedProtocol<Input>> {
    let input_handle = boot::get_handle_for_protocol::<Input>().ok()?;
    boot::open_protocol_exclusive::<Input>(input_handle).ok()
}

/// Set OS indications so that a reboot will boot into setup menu
pub fn set_os_indications_setupmenu() {
    let vendor = VariableVendor(EFI_GLOBAL_VARIABLE_GUID);
    let mut buf = [0; 8];
    uefi::runtime::get_variable(cstr16!("OsIndicationsSupported"), &vendor, &mut buf)
        .expect("failed to get variable");
    let os_indications: u64 = u64::from_ne_bytes(buf);
    debug!("OS Indications: {:?}", os_indications);

    let os_indications =
        if uefi::runtime::get_variable(cstr16!("OsIndications"), &vendor, &mut buf).is_ok() {
            u64::from_ne_bytes(buf) | EFI_OS_INDICATIONS_BOOT_TO_FW_UI
        } else {
            EFI_OS_INDICATIONS_BOOT_TO_FW_UI
        };
    debug!("OS Indications: {:?}", os_indications);

    let attrs = VariableAttributes::NON_VOLATILE
        | VariableAttributes::BOOTSERVICE_ACCESS
        | VariableAttributes::RUNTIME_ACCESS;
    uefi::runtime::set_variable(
        cstr16!("OsIndications"),
        &vendor,
        attrs,
        &u64::to_ne_bytes(os_indications),
    )
    .expect("failed to set variable");
}
