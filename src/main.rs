#![no_main]
#![no_std]
#![allow(clippy::result_unit_err)]

extern crate alloc;

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

mod gui;

#[entry]
fn main(image_handle: Handle, mut system_table: SystemTable<Boot>) -> Status {
    uefi_services::init(&mut system_table).unwrap();
    let boot_services = system_table.boot_services();

    #[cfg(not(feature = "qemu"))]
    log::set_max_level(log::LevelFilter::Error);

    if let Ok(true) = has_shell_args(boot_services) {
        print_module_status();
    } else {
        info!("No shell arg, launching GUI");
        gui::draw_background(boot_services, &system_table).unwrap();
    }

    Status::SUCCESS
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
