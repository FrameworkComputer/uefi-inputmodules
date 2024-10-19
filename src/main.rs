#![no_main]
#![no_std]
#![allow(clippy::result_unit_err)]

extern crate alloc;

use alloc::string::ToString;
use framework_lib::chromium_ec::commands::*;
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
#[cfg(not(feature = "12in"))]
mod drawing;
#[cfg(feature = "12in")]
mod drawing_12in;
mod ec_comm;
// Allow dead code because it's the old implementation that we will remove
#[allow(dead_code)]
#[cfg(not(feature = "12in"))]
mod gui;
#[cfg(feature = "12in")]
mod gui_12in;
mod sleep;
#[cfg(not(feature = "12in"))]
mod state_machine;
#[cfg(feature = "12in")]
mod state_machine_12in;
mod util;
mod built_info {
    // The file has been placed there by the build script.
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

#[entry]
fn main(image_handle: Handle, mut system_table: SystemTable<Boot>) -> Status {
    let event = uefi_services::init(&mut system_table).unwrap();
    let boot_services = system_table.boot_services();

    // Only error prints in release build
    #[cfg(not(debug_assertions))]
    log::set_max_level(log::LevelFilter::Error);
    log::set_max_level(log::LevelFilter::Error);
    //#[cfg(debug_assertions)]
    //log::set_max_level(log::LevelFilter::Trace);

    let arg = get_shell_action(boot_services);
    if let Ok(ShellAction::Info) = arg {
        println!("Provide Parameter: 'gui' to launch UI and show issues");
        println!("Provide Parameter: 'force' to launch UI even if everything is ok");
        println!();
        #[cfg(not(feature = "12in"))]
        print_module_status();
        #[cfg(feature = "12in")]
        print_12in_status();
    } else {
        info!("No shell arg, launching GUI");

        #[cfg(not(feature = "qemu"))]
        let ec = Some(CrosEc::new());
        #[cfg(feature = "qemu")]
        let ec = None;

        ec_comm::set_uefi_app_mode(&ec, true);
        let quick_boot = !matches!(arg, Ok(ShellAction::Force));
        #[cfg(not(feature = "12in"))]
        let gui_res = state_machine::start(boot_services, &system_table, &ec, quick_boot);
        #[cfg(feature = "12in")]
        let gui_res = state_machine_12in::start(boot_services, &system_table, &ec, quick_boot);
        ec_comm::set_uefi_app_mode(&ec, false);

        let reset = gui_res.unwrap();
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

fn print_module_status() {
    print_tool_version();
    println!("");

    let ec  = CrosEc::new();

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
}

fn print_12in_status() {
    print_tool_version();
    println!("");

    framework_lib::commandline::smbios_info();

    println!("Connecting to Sunflower EC");
    #[cfg(not(feature = "qemu"))]
    let ec = CrosEc::new();
    #[cfg(feature = "qemu")]
    let ec  = CrosEc::new();

    // TODO: Don't unwrap
    println!("Trying to get intrusion status");
    let intrusion = ec.get_intrusion_status().unwrap();
    let hw_diag = ec_comm::get_hw_diag(&Some(ec)).hw_diag;
    let no_audio = hw_diag & (1 << DIAGNOSTICS_AUDIO_DAUGHTERBOARD) != 0;
    let no_fan = hw_diag & (1 << DIAGNOSTICS_NOFAN) != 0;
    let no_touchpad = hw_diag & (1 << DIAGNOSTICS_TOUCHPAD) != 0;
    let input_deck_ok = !intrusion.currently_open && !no_touchpad;

    println!("Input Deck Status:");
    println!("  Chassis Closed:    {}", !intrusion.currently_open);
    println!("  Touchpad Present:  {:?}", !no_touchpad);
    // TODO: I2C checks
    println!("  Touchpad I2C OK:   {:?}", true);
    println!("  Keyboard I2C OK:   {:?}", true);
    println!("  Overall OK:        {:?}", input_deck_ok);

    println!("Audio Board");
    println!("  Present:           {:?}", !no_audio);

    println!("Camera");
    // TODO: G-Sensor BOARD_ID
    println!("  Present:           {:?}", true);

    println!("CPU Fan");
    println!("  Present:           {:?}", !no_fan);
}

fn print_tool_version() {
    let q = "?".to_string();
    println!("Tool Version Information");
    println!("  Version:     {}", built_info::PKG_VERSION);
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
