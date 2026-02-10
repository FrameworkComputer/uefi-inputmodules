#![no_main]
#![no_std]
#![allow(clippy::result_unit_err)]

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use framework_lib::chromium_ec::input_deck::InputDeckState;
use framework_lib::chromium_ec::{print_err, CrosEc};
#[allow(unused_imports)]
use log::{debug, error, info, trace};
use uefi::boot::{self, OpenProtocolAttributes, OpenProtocolParams, SearchType};
use uefi::prelude::*;
use uefi::proto::shell_params::ShellParameters;
use uefi::Identify;
#[allow(unused_imports)]
use uefi::{print, println};

mod buffer;
mod drawing;
mod ec_comm;
mod oem_variable;
mod sleep;
mod state_machine;
mod uefi_usb;
mod built_info {
    // The file has been placed there by the build script.
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

struct BiosSettings {
    standalone_mode: bool,
    inputmodule_setting: Option<oem_variable::InputmoduleSetting>,
}

fn get_bios_settings() -> Option<BiosSettings> {
    // If the protocol isn't present, assume nothing is disabled and show the UI
    let oem_handle = boot::get_handle_for_protocol::<oem_variable::UefiVariable>().ok()?;
    let oem = boot::open_protocol_exclusive::<oem_variable::UefiVariable>(oem_handle).ok()?;

    Some(BiosSettings {
        standalone_mode: oem.standalone_mode_enabled(),
        inputmodule_setting: oem.inputmodule_setting(),
    })
}

/// If either standalone mode is enabled or Require Inputmodules is set to ForceOff/ForceOn,
/// don't show the UI but immediately exit and continue booting
/// Return value of Some(false) means don't show, everything else means show
///
/// This is how the BIOS was previously doing it.
/// TODO: Actually we want to show the UI in any case, if:
/// - dGPU PD is outdated
/// - Fan has issues
/// - Interposer or door has issues
fn show_ui() -> Option<bool> {
    let settings = get_bios_settings()?;
    let require_inputmodules =
        settings.inputmodule_setting == Some(oem_variable::InputmoduleSetting::RequireModules);

    // require_inputmodules should be equal to the below predicate
    // !matches!(input_deck.state, InputDeckState::Disconnected | InputDeckState::TurningOn | InputDeckState::On)

    Some(!settings.standalone_mode && require_inputmodules)
}

/// Since Rust 1.99 (LLVM 23), loops that search for a NUL u16, like in
/// `CStr16::from_ptr`, are turned into calls to wcslen.
/// There's no libc on UEFI and compiler_builtins doesn't provide it.
///
/// # Safety
/// `s` must point to a NUL-terminated UCS-2 string
#[no_mangle]
pub unsafe extern "C" fn wcslen(s: *const u16) -> usize {
    let mut len = 0;
    // Volatile read, so LLVM can't turn this loop into a call to itself
    while core::ptr::read_volatile(s.add(len)) != 0 {
        len += 1;
    }
    len
}

#[entry]
fn main() -> Status {
    uefi::helpers::init().unwrap();

    // Only error prints in release build
    #[cfg(not(debug_assertions))]
    log::set_max_level(log::LevelFilter::Error);

    let arg = get_shell_action();
    if let Ok(ShellAction::Info) = arg {
        println!("Provide Parameter: 'gui' to launch UI and show issues");
        println!("Provide Parameter: 'force' to launch UI even if everything is ok");
        println!("Provide Parameter: 'gop' to print GOP debugging info");
        println!();
        print_module_status();
    } else if let Ok(ShellAction::GopInfo) = arg {
        println!("GOP Info");
        state_machine::print_gop_info();
    } else {
        info!("No shell arg, launching GUI");

        #[cfg(not(feature = "qemu"))]
        let ec = Some(CrosEc::new());
        #[cfg(feature = "qemu")]
        let ec = None;

        let force_ui = matches!(arg, Ok(ShellAction::Force));

        // Show the UI if we can't tell the BIOS settings or they indicate we need to show the UI
        let show_ui = show_ui() != Some(false);

        let reset = if force_ui || show_ui {
            ec_comm::set_uefi_app_mode(&ec, true);
            let gui_res = state_machine::start(&ec, force_ui);
            ec_comm::set_uefi_app_mode(&ec, false);
            gui_res.unwrap()
        } else {
            // If didn't run the app, don't reset, just keep booting
            None
        };

        if let Some(reset_type) = reset {
            #[cfg(debug_assertions)]
            info!("Shutting down");
            boot::stall(core::time::Duration::from_micros(1_000_000));

            // Shut down the system
            // TODO: I should do this outside of this function
            uefi::runtime::reset(reset_type, Status::SUCCESS, None);
        }
    }

    Status::SUCCESS
}

#[derive(Debug)]
enum ShellAction {
    Gui,
    Force,
    Info,
    GopInfo,
}

fn get_shell_action() -> uefi::Result<ShellAction> {
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

        let args: Vec<String> = params_handle.args().map(|arg| arg.to_string()).collect();
        // First arg is the executable path
        return Ok(if args.len() == 2 && args[1] == "gui" {
            ShellAction::Gui
        } else if args.len() == 2 && args[1] == "force" {
            ShellAction::Force
        } else if args.len() == 2 && args[1] == "gop" {
            ShellAction::GopInfo
        } else if args.len() == 1 {
            ShellAction::Gui
        } else {
            ShellAction::Info
        });
    }
    Ok(ShellAction::Gui)
}

fn print_module_status() {
    print_tool_version();
    println!("");

    let ec = CrosEc::new();

    println!("Input Module Status:");
    if let Some(status) = print_err(ec.get_input_deck_status()) {
        println!("  Input Deck State: {:?}", status.state);
        if !matches!(
            status.state,
            InputDeckState::Disconnected | InputDeckState::TurningOn | InputDeckState::On
        ) {
            println!(
                "  Input Deck State Overridden: {:?}. No detection of modules possible",
                status.state
            );
        }
        println!("  Hubboard present: {:?}", status.hubboard_present);
        println!("  Touchpad present: {:?}", status.touchpad_present);
        println!("  Positions:");
        println!("    Pos 0: {:?}", status.top_row.pos0);
        println!("    Pos 1: {:?}", status.top_row.pos1);
        println!("    Pos 2: {:?}", status.top_row.pos2);
        println!("    Pos 3: {:?}", status.top_row.pos3);
        println!("    Pos 4: {:?}", status.top_row.pos4);
    } else {
        println!("  Unable to tell");
    }

    println!("  Firmware Versions");
    if let Ok(devices) = uefi_usb::get_usb_devices() {
        for device in devices {
            let name = if let Some(name) = device.get_name() {
                name
            } else {
                continue;
            };

            println!("    {}: {}", name, device.format_bcd_device());
        }
    } else {
        println!("    Unknown");
    }

    println!("BIOS Settings");
    if let Some(settings) = get_bios_settings() {
        println!("  Standalone Mode Enabled: {}", settings.standalone_mode);
        println!(
            "  Inputmodule Setting:     {:?}",
            settings.inputmodule_setting
        );
    } else {
        println!("  Unknown");
    }
}

fn print_tool_version() {
    let q = "?".to_string();
    println!("Tool Version Information");
    println!("  Version:     {}", built_info::PKG_VERSION);
    #[cfg(debug_assertions)]
    println!("  Built At:    {}", built_info::BUILT_TIME_UTC);
    println!(
        "  Git Commit:  {}",
        built_info::GIT_COMMIT_HASH.unwrap_or(&q)
    );
    println!(
        "  Git Dirty:   {}",
        built_info::GIT_DIRTY
            .map(|x| x.to_string())
            .unwrap_or(q.clone())
    );

    println!(
        "  Built on CI: {:?}",
        built_info::CI_PLATFORM.unwrap_or("None")
    );
    println!(
        "  Git ref:     {:?}",
        built_info::GIT_HEAD_REF.unwrap_or(&q)
    );
    println!("  rustc Ver:   {}", built_info::RUSTC_VERSION);
    println!("  Features     {:?}", built_info::FEATURES);
    println!("  DEBUG:       {}", built_info::DEBUG);
    println!("  Target OS:   {}", built_info::CFG_OS);
}
