#![no_main]
#![no_std]
#![allow(clippy::result_unit_err)]

extern crate alloc;

use alloc::string::ToString;
use framework_lib::chromium_ec::input_deck::InputDeckState;
use framework_lib::chromium_ec::{print_err, CrosEc};
#[allow(unused_imports)]
use log::{debug, error, info, trace};
use uefi::prelude::*;
use uefi::proto::shell_params::*;
use uefi::table::boot::{OpenProtocolAttributes, OpenProtocolParams, SearchType};
use uefi::Identify;
#[allow(unused_imports)]
use uefi_services::{print, println};

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

fn get_bios_settings(boot_services: &BootServices) -> Option<BiosSettings> {
    // If the protocol isn't present, assume nothing is disabled and show the UI
    let oem_handle = boot_services
        .get_handle_for_protocol::<oem_variable::UefiVariable>()
        .ok()?;
    let oem = boot_services
        .open_protocol_exclusive::<oem_variable::UefiVariable>(oem_handle)
        .ok()?;

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
fn show_ui(boot_services: &BootServices) -> Option<bool> {
    let settings = get_bios_settings(boot_services)?;
    let require_inputmodules =
        settings.inputmodule_setting == Some(oem_variable::InputmoduleSetting::RequireModules);

    // require_inputmodules should be equal to the below predicate
    // !matches!(input_deck.state, InputDeckState::Disconnected | InputDeckState::TurningOn | InputDeckState::On)

    Some(!settings.standalone_mode && require_inputmodules)
}

#[entry]
fn main(image_handle: Handle, mut system_table: SystemTable<Boot>) -> Status {
    let event = uefi_services::init(&mut system_table).unwrap();
    let boot_services = system_table.boot_services();

    // Only error prints in release build
    #[cfg(not(debug_assertions))]
    log::set_max_level(log::LevelFilter::Error);

    let arg = get_shell_action(boot_services);
    if let Ok(ShellAction::Info) = arg {
        println!("Provide Parameter: 'gui' to launch UI and show issues");
        println!("Provide Parameter: 'force' to launch UI even if everything is ok");
        println!();
        print_module_status(&boot_services);
    } else {
        info!("No shell arg, launching GUI");

        #[cfg(not(feature = "qemu"))]
        let ec = Some(CrosEc::new());
        #[cfg(feature = "qemu")]
        let ec = None;

        let force_ui = matches!(arg, Ok(ShellAction::Force));

        // Show the UI if we can't tell the BIOS settings or they indicate we need to show the UI
        let show_ui = show_ui(boot_services) != Some(false);

        let reset = if force_ui || show_ui {
            ec_comm::set_uefi_app_mode(&ec, true);
            let gui_res = state_machine::start(boot_services, &system_table, &ec, force_ui);
            ec_comm::set_uefi_app_mode(&ec, false);
            gui_res.unwrap()
        } else {
            // If didn't run the app, don't reset, just keep booting
            None
        };

        if let Some(reset_type) = reset {
            #[cfg(debug_assertions)]
            info!("Shutting down");
            boot_services.stall(1_000_000);

            // Shut down the system
            // TODO: I should do this outside of this function
            let rt = system_table.runtime_services();
            rt.reset(reset_type, Status::SUCCESS, None);
        }
    }

    if let Some(event) = event {
        // Will crash when Linux boots if we don't call this
        boot_services.close_event(event).unwrap();
    }

    Status::SUCCESS
}

#[derive(Debug)]
enum ShellAction {
    Gui,
    Force,
    Info,
}

fn get_shell_action(boot_services: &BootServices) -> uefi::Result<ShellAction> {
    // TODO: I think I should open this from the ImageHandle?
    let shell_params_h =
        boot_services.locate_handle_buffer(SearchType::ByProtocol(&ShellParameters::GUID))?;
    trace!("Found {} ShellParameters handles", (*shell_params_h).len());
    for handle in &*shell_params_h {
        let params_handle = unsafe {
            boot_services.open_protocol::<ShellParameters>(
                OpenProtocolParams {
                    handle: *handle,
                    agent: boot_services.image_handle(),
                    controller: None,
                },
                OpenProtocolAttributes::GetProtocol,
            )?
        };

        // Ehm why are there two and one has no args?
        // Maybe one is the shell itself?
        if params_handle.argc == 0 {
            continue;
        }

        let args = params_handle.get_args();
        // First arg is the executable path
        return Ok(if args.len() == 2 && args[1] == "gui" {
            ShellAction::Gui
        } else if args.len() == 2 && args[1] == "force" {
            ShellAction::Force
        } else if args.len() == 1 {
            ShellAction::Gui
        } else {
            ShellAction::Info
        });
    }
    Ok(ShellAction::Gui)
}

fn print_module_status(boot_services: &BootServices) {
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
    if let Ok(devices) = uefi_usb::get_usb_devices(boot_services) {
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
    if let Some(settings) = get_bios_settings(boot_services) {
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
