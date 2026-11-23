//! Laptop 16 BIOS settings and console status output
use framework_lib::chromium_ec::input_deck::InputDeckState;
use framework_lib::chromium_ec::{print_err, CrosEc};
use uefi::boot;
use uefi::println;

use super::oem_variable;
use crate::uefi_usb;

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
pub fn show_ui() -> Option<bool> {
    let settings = get_bios_settings()?;
    let require_inputmodules =
        settings.inputmodule_setting == Some(oem_variable::InputmoduleSetting::RequireModules);

    // require_inputmodules should be equal to the below predicate
    // !matches!(input_deck.state, InputDeckState::Disconnected | InputDeckState::TurningOn | InputDeckState::On)

    Some(!settings.standalone_mode && require_inputmodules)
}

pub fn print_module_status() {
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
