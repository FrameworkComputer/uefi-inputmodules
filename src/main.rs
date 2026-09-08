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
use uefi::table::runtime::RuntimeServices;
use uefi::Identify;
#[allow(unused_imports)]
use uefi_services::{print, println};

mod buffer;
mod drawing;
mod ec_comm;
mod oem_variable;
mod run_record;
mod sleep;
mod state_machine;
mod timestamp;
mod uefi_usb;
mod built_info {
    // The file has been placed there by the build script.
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

struct BiosSettings {
    standalone_mode: bool,
    inputmodule_setting: Option<oem_variable::InputmoduleSetting>,
    /// Raw values, to save in the run record
    standalone_mode_raw: u8,
    inputmodule_setting_raw: u8,
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
        standalone_mode_raw: oem.standalone_mode_raw(),
        inputmodule_setting_raw: oem.inputmodule_setting_raw(),
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
fn show_ui(boot_services: &BootServices, record: &mut run_record::RunRecord) -> Option<bool> {
    let settings = match get_bios_settings(boot_services) {
        Some(settings) => settings,
        None => {
            record.set(run_record::flags::NO_BIOS_SETTINGS);
            return None;
        }
    };
    record.bios_standalone_mode = settings.standalone_mode_raw;
    record.bios_inputmodule_setting = settings.inputmodule_setting_raw;

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

    // Optional, so we may not be able to tell how long we took
    let stopwatch = timestamp::Stopwatch::start(boot_services);

    // Only error prints in release build.
    // Before anything that logs, otherwise we print to the screen during boot
    #[cfg(not(debug_assertions))]
    log::set_max_level(log::LevelFilter::Error);

    // Save that we started right away. If the OS finds a record without the
    // Finished flag, we crashed or hung somewhere below
    let mut record = run_record::RunRecord::new();
    record.write(system_table.runtime_services());

    let arg = get_shell_action(boot_services);
    if let Ok(ShellAction::Info) = arg {
        println!("Provide Parameter: 'gui' to launch UI and show issues");
        println!("Provide Parameter: 'force' to launch UI even if everything is ok");
        println!("Provide Parameter: 'gop' to print GOP debugging info");
        println!();
        record.set(run_record::flags::SHELL_INFO_MODE);
        print_module_status(&boot_services);
    } else if let Ok(ShellAction::GopInfo) = arg {
        println!("GOP Info");
        record.set(run_record::flags::SHELL_INFO_MODE);
        state_machine::print_gop_info(boot_services);
    } else {
        info!("No shell arg, launching GUI");

        #[cfg(not(feature = "qemu"))]
        let ec = Some(CrosEc::new());
        #[cfg(feature = "qemu")]
        let ec = None;

        let force_ui = matches!(arg, Ok(ShellAction::Force));
        if force_ui {
            record.set(run_record::flags::FORCE_UI);
        }

        // Show the UI if we can't tell the BIOS settings or they indicate we need to show the UI
        let show_ui = show_ui(boot_services, &mut record) != Some(false);
        if !show_ui {
            record.set(run_record::flags::SKIPPED_BY_BIOS_SETTING);
        }

        let reset = if force_ui || show_ui {
            ec_comm::set_uefi_app_mode(&ec, true);
            let gui_res =
                state_machine::start(boot_services, &system_table, &ec, force_ui, &mut record);
            ec_comm::set_uefi_app_mode(&ec, false);
            gui_res.unwrap()
        } else {
            // If didn't run the app, don't reset, just keep booting
            None
        };

        if let Some(reset_type) = reset {
            // Save before stalling, otherwise the stall is part of the runtime.
            // The record is volatile, so the reset below wipes it. Nothing we
            // can do about that, but a user initiated exit is not the case we
            // are trying to debug anyway.
            record.set_reset_type(reset_type);
            finish(&mut record, &stopwatch, system_table.runtime_services());

            #[cfg(debug_assertions)]
            info!("Shutting down");
            boot_services.stall(1_000_000);

            // Shut down the system
            // TODO: I should do this outside of this function
            let rt = system_table.runtime_services();
            rt.reset(reset_type, Status::SUCCESS, None);
        }
    }

    finish(&mut record, &stopwatch, system_table.runtime_services());

    if let Some(event) = event {
        // Will crash when Linux boots if we don't call this
        boot_services.close_event(event).unwrap();
    }

    Status::SUCCESS
}

/// Fill in how long we took and save the record for the OS to read
fn finish(
    record: &mut run_record::RunRecord,
    stopwatch: &Option<timestamp::Stopwatch>,
    rt: &RuntimeServices,
) {
    match stopwatch {
        Some(stopwatch) => {
            let duration_us = stopwatch.elapsed_us();
            record.duration_us = duration_us;
            record.set(run_record::flags::HAVE_DURATION);
            info!("Ran for {} us", duration_us);
        }
        None => info!("No EFI_TIMESTAMP_PROTOCOL, can't tell how long we ran"),
    }

    // We always return SUCCESS, so that the boot manager keeps booting
    record.set_exit_status(Status::SUCCESS);
    record.set(run_record::flags::FINISHED);
    record.write(rt);
}

#[derive(Debug)]
enum ShellAction {
    Gui,
    Force,
    Info,
    GopInfo,
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

    println!("Timestamp Protocol");
    if let Some(props) = timestamp::get_properties(boot_services) {
        println!("  Frequency:   {} Hz", props.frequency);
        println!("  End Value:   {:#x}", props.end_value);
        println!(
            "  Rolls over:  After {} s",
            props.end_value / props.frequency.max(1)
        );
    } else {
        println!("  Not supported");
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
