#![no_main]
#![no_std]
#![allow(clippy::result_unit_err)]

extern crate alloc;

use alloc::string::ToString;
use framework_lib::chromium_ec::commands::EcRequestUefiAppMode;
use framework_lib::chromium_ec::input_deck::InputDeckState;
use framework_lib::chromium_ec::{print_err, CrosEc, EcRequestRaw};
#[allow(unused_imports)]
use log::{debug, error, info, trace};
use uefi::prelude::*;
use uefi::proto::shell_params::*;
use uefi::table::boot::{OpenProtocolAttributes, OpenProtocolParams, SearchType};
use uefi::Identify;
#[allow(unused_imports)]
use uefi_services::{print, println};

use crate::state_machine::set_uefi_app_mode;

mod buffer;
mod drawing;
mod gui;
mod state_machine;
mod built_info {
    // The file has been placed there by the build script.
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

#[entry]
fn main(image_handle: Handle, mut system_table: SystemTable<Boot>) -> Status {
    let event = uefi_services::init(&mut system_table).unwrap();
    let boot_services = system_table.boot_services();

    #[cfg(not(feature = "qemu"))]
    log::set_max_level(log::LevelFilter::Error);

    if let Ok(true) = has_shell_args(boot_services) {
        print_module_status();
    } else {
        info!("No shell arg, launching GUI");

        #[cfg(not(feature = "qemu"))]
        let ec = Some(CrosEc::new());
        #[cfg(feature = "qemu")]
        let ec = None;

        set_uefi_app_mode(&ec, true);
        let gui_res = state_machine::start(boot_services, &system_table, &ec);
        set_uefi_app_mode(&ec, false);

        let poweroff = gui_res.unwrap();
        if poweroff {
            // TODO: Remove debug print and stall
            println!("Shutting down;");
            boot_services.stall(1_000_000);

            // Shut down the system
            // TODO: I should do  this outside of this function
            let rt = system_table.runtime_services();
            rt.reset(
                uefi::table::runtime::ResetType::Shutdown,
                Status::SUCCESS,
                None,
            );
        }
    }

    if let Some(event) = event {
        // Will crash when Linux boots if we don't call this
        boot_services.close_event(event).unwrap();
    }

    return Status::SUCCESS;
    // Put here to avoid unused warning
    let _ = gui::draw_background(boot_services, &system_table, &None);
}

fn has_shell_args(boot_services: &BootServices) -> uefi::Result<bool> {
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
        return Ok(args.len() > 1);
    }
    Ok(false)
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
