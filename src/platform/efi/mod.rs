//! UEFI backend: Draw with GOP, read keys from the console input protocol
//! and use UEFI runtime services for time, variables and reset.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::time::Duration;

#[allow(unused_imports)]
use log::{debug, error, info, trace};
use uefi::boot::{self, OpenProtocolAttributes, OpenProtocolParams, ScopedProtocol, SearchType};
use uefi::println;
use uefi::proto::console::gop::{BltOp, BltRegion, GraphicsOutput};
use uefi::proto::console::text::{Input, Key};
use uefi::proto::shell_params::ShellParameters;
use uefi::runtime::{ResetType, VariableAttributes, VariableVendor};
use uefi::{cstr16, guid, Identify, Status};

use crate::buffer::Buffer;
use crate::oem_variable;
use crate::BiosSettings;

mod sleep;
mod usb;

use sleep::sleep_or;
pub use usb::get_usb_devices;

const FAILED_INPUT: &str = "Failed to get keypress";

const EFI_OS_INDICATIONS_BOOT_TO_FW_UI: u64 = 0x0000000000000001;
const EFI_GLOBAL_VARIABLE_GUID: uefi::Guid = guid!("8be4df61-93ca-11d2-aa0d-00e098032b8c");

/// Display (GOP) and keyboard (console input)
pub struct Screen {
    gop: ScopedProtocol<GraphicsOutput>,
    input: ScopedProtocol<Input>,
}

impl Screen {
    /// Open GOP and Input protocols
    ///
    /// Returns None if there is no display to draw on
    pub fn open() -> Option<Self> {
        let gop = get_gop()?;
        let input = get_input_protocol().expect("Should always have input protocol");
        Some(Screen { gop, input })
    }

    /// Resolution of the display
    pub fn size(&self) -> (usize, usize) {
        let (width, height) = self.gop.current_mode_info().resolution();
        info!("Current Resolution {}x{}", width, height);
        (width, height)
    }

    /// Blit the buffer to the framebuffer
    pub fn blit(&mut self, buffer: &Buffer) -> uefi::Result {
        self.gop.blt(BltOp::BufferToVideo {
            buffer: buffer.pixels(),
            src: BltRegion::Full,
            dest: (0, 0),
            dims: (buffer.width(), buffer.height()),
        })
    }

    /// Sleep until a key is pressed or the timeout expires, without consuming the key
    pub fn wait(&mut self, ms: u64) {
        sleep_or(ms, self.input.wait_for_key_event().ok());
    }

    /// Sleep until a key is pressed or the timeout expires, then return the pressed key, if any
    pub fn wait_for_key(&mut self, ms: u64) -> Option<Key> {
        self.wait(ms);
        self.input.read_key().expect(FAILED_INPUT)
    }
}

fn get_gop() -> Option<ScopedProtocol<GraphicsOutput>> {
    let gop_handles = if let Ok(gop_handles) =
        boot::locate_handle_buffer(SearchType::ByProtocol(&GraphicsOutput::GUID))
    {
        gop_handles
    } else {
        error!("GraphicsOutput protocol is missing");
        return None;
    };

    if let Some(handle) = gop_handles.iter().next() {
        Some(unsafe {
            boot::open_protocol::<GraphicsOutput>(
                OpenProtocolParams {
                    handle: *handle,
                    agent: boot::image_handle(),
                    controller: None,
                },
                OpenProtocolAttributes::GetProtocol,
            )
            .expect("Failed to open GraphicsOutput handle")
        })
    } else {
        error!("No GOP handles found");
        None
    }
}

fn get_input_protocol() -> Option<ScopedProtocol<Input>> {
    let input_handles = if let Ok(input_handles) =
        boot::locate_handle_buffer(SearchType::ByProtocol(&Input::GUID))
    {
        input_handles
    } else {
        error!("Input protocol is missing");
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

pub fn print_gop_info() -> Option<()> {
    let gop = get_gop()?;

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

/// Busy-wait for the given duration
pub fn stall(duration: Duration) {
    boot::stall(duration);
}

/// Seconds since the start of the current hour, according to the RTC
pub fn clock_seconds() -> Option<u32> {
    let now = uefi::runtime::get_time().ok()?;
    Some(now.second() as u32 + (now.minute() as u32 * 60))
}

/// Reset or shut down the system. Does not return.
pub fn reset(reset_type: ResetType) {
    #[cfg(debug_assertions)]
    info!("Shutting down");
    boot::stall(Duration::from_micros(1_000_000));

    uefi::runtime::reset(reset_type, Status::SUCCESS, None);
}

/// Force a cold reset. Does not return.
///
/// EfiResetCold does a warm reset on AMD systems,
/// so we need to manually force a cold reset.
/// Use legacy CF9 register - AMD doc says it keeps CPU in S5 for 3 seconds.
pub fn force_cold_reset() {
    use hwio::{Io, Pio};
    Pio::<u8>::new(0x0CF9).write(0x0E);
    // If it reaches here, it will panic, show an error message and shut off
    // It's safe and customers can report this issue, if it occurs
    unreachable!();
}

/// Tell the EC that the UEFI app is running (or not anymore)
pub fn set_uefi_app_mode(ec: &Option<framework_lib::chromium_ec::CrosEc>, mode: bool) {
    crate::ec_comm::set_uefi_app_mode(ec, mode);
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

pub fn get_bios_settings() -> Option<BiosSettings> {
    // If the protocol isn't present, assume nothing is disabled and show the UI
    let oem_handle = boot::get_handle_for_protocol::<oem_variable::UefiVariable>().ok()?;
    let oem = boot::open_protocol_exclusive::<oem_variable::UefiVariable>(oem_handle).ok()?;

    Some(BiosSettings {
        standalone_mode: oem.standalone_mode_enabled(),
        inputmodule_setting: oem.inputmodule_setting(),
    })
}

/// Get the commandline arguments from the UEFI shell
///
/// First argument is the executable path.
/// Returns None if not launched from the shell.
pub fn get_args() -> uefi::Result<Option<Vec<String>>> {
    // TODO: I think I should open this from the ImageHandle?
    let shell_params_h =
        boot::locate_handle_buffer(SearchType::ByProtocol(&ShellParameters::GUID))?;
    trace!("Found {} ShellParameters handles", shell_params_h.len());
    for handle in shell_params_h.iter() {
        let params_handle = unsafe {
            boot::open_protocol::<ShellParameters>(
                OpenProtocolParams {
                    handle: *handle,
                    agent: boot::image_handle(),
                    controller: None,
                },
                OpenProtocolAttributes::GetProtocol,
            )?
        };

        // Ehm why are there two and one has no args?
        // Maybe one is the shell itself?
        if params_handle.args_len() == 0 {
            continue;
        }

        return Ok(Some(
            params_handle.args().map(|arg| arg.to_string()).collect(),
        ));
    }
    Ok(None)
}
