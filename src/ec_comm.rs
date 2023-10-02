use framework_lib::chromium_ec::commands::{
    EcRequestExpansionBayStatus, EcRequestGetHwDiag, EcRequestUefiAppBtnStatus,
    EcRequestUefiAppMode, EcResponseExpansionBayStatus, EcResponseGetHwDiag, ExpansionByStates,
    BOARD_VERSION_12,
};
use framework_lib::chromium_ec::input_deck::{self, InputDeckStatus};
use framework_lib::chromium_ec::input_deck::{InputDeckState, InputModuleType, TopRowPositions};
use framework_lib::chromium_ec::{CrosEc, EcError, EcRequestRaw, EcResponseStatus, EcResult};
#[allow(unused_imports)]
use log::{debug, error, info, trace};
use uefi::proto::console::text::Key;
use uefi::Char16;

/// Get current state of expansion bay
///
/// If error or command not supported (early EC version), mock as OK, to avoid preventing boot.
/// If no EC => not fully populated to allow for testing
pub fn get_expansion_bay(ec: &Option<CrosEc>) -> EcResponseExpansionBayStatus {
    let all_ok = EcResponseExpansionBayStatus {
        state: (ExpansionByStates::ModuleEnabled as u8)
            | (ExpansionByStates::HatchSwitchClosed as u8),
        // Double 12 is dual interposer
        board_id_0: BOARD_VERSION_12,
        board_id_1: BOARD_VERSION_12,
    };
    let not_ok = EcResponseExpansionBayStatus {
        state: 0,
        board_id_0: 0,
        board_id_1: 0,
    };

    // Pretend there's no EC because the EC is guaranteed not to have this command
    #[cfg(feature = "azalea")]
    let ec = None;

    let ec = if let Some(ec) = ec {
        ec
    } else {
        // If not EC, fake missing module so we can test the app
        return not_ok;
    };

    match (EcRequestExpansionBayStatus {}.send_command(ec)) {
        Ok(expansion_bay) => expansion_bay,
        // Old EC version
        EcResult::Err(EcError::Response(EcResponseStatus::InvalidCommand)) => all_ok,
        // Other error or no EC
        _ => all_ok,
    }
}

/// Get current state of hubboard and input deck
///
/// If no EC => unpopulated
/// If any error => populated
/// This will make sure the system can always boot,
/// but on testing systems (QEMU, 13inch) it will show an issue to help debug.
pub fn get_input_deck(ec: &Option<CrosEc>) -> InputDeckStatus {
    let populated = InputDeckStatus {
        state: InputDeckState::On,
        hubboard_present: true,
        touchpad_present: true,
        top_row: TopRowPositions {
            pos0: InputModuleType::KeyboardA,
            pos1: InputModuleType::Disconnected,
            pos2: InputModuleType::Disconnected,
            pos3: InputModuleType::KeyboardB,
            pos4: InputModuleType::Disconnected,
        },
    };
    let no_hubboard = InputDeckStatus {
        state: InputDeckState::On,
        hubboard_present: false,
        touchpad_present: true,
        top_row: TopRowPositions {
            pos0: InputModuleType::KeyboardA,
            pos1: InputModuleType::Disconnected,
            pos2: InputModuleType::Disconnected,
            pos3: InputModuleType::KeyboardB,
            pos4: InputModuleType::Disconnected,
        },
    };

    // Pretend there's no EC because the EC is guaranteed not to have this command
    #[cfg(feature = "azalea")]
    let ec = None;

    let ec = if let Some(ec) = ec {
        ec
    } else {
        // If not EC, fake missing module so we can test the app
        return no_hubboard;
    };

    let input_deck = ec.get_input_deck_status();
    match input_deck {
        Ok(input_deck) => input_deck,
        // Old EC version
        EcResult::Err(EcError::Response(EcResponseStatus::InvalidCommand)) => populated,
        // Other error or no EC
        _ => populated,
    }
}

/// Ask EC if the power button is pressed right now
///
/// If command not supported (early EC version), mock as false.
/// If other error also return false.
/// If no EC (QEMU) also return false.
/// false is a safe default.
pub fn button_pressed(ec: &Option<CrosEc>) -> bool {
    if let Some(ref ec) = ec {
        // TODO: Maybe check some other errors
        match (EcRequestUefiAppBtnStatus {}.send_command(ec)) {
            Ok(btn_status) => btn_status.status == 0,
            // On old EC version
            EcResult::Err(EcError::Response(EcResponseStatus::InvalidCommand)) => false,
            // Other error
            _ => false,
        }
    } else {
        false
    }
}

/// Get Hardware Diagnostics from EC
///
/// If no EC => flag HW diag
/// If any error => no HW diag error
/// This will make sure the system can always boot,
/// but on testing systems (QEMU, 13inch) it will show an issue to help debug.
pub fn get_hw_diag(ec: &Option<CrosEc>) -> EcResponseGetHwDiag {
    let not_ok = EcResponseGetHwDiag {
        // All HW diags flagged (more than possible in reality)
        hw_diag: 0xFF,
        // BIOS not complete, since it's a UEFI app
        bios_complete: 0,
    };
    let all_ok = EcResponseGetHwDiag {
        // No HW diag issue
        hw_diag: 0,
        // BIOS not complete, since it's a UEFI app
        bios_complete: 0,
    };

    // Pretend there's no EC because the EC is guaranteed not to have this command
    #[cfg(feature = "azalea")]
    let ec = None;

    let ec = if let Some(ec) = ec {
        ec
    } else {
        // If not EC, fake missing module so we can test the app
        return not_ok;
    };

    match (EcRequestGetHwDiag {}.send_command(ec)) {
        Ok(diag) => diag,
        // Old EC version
        EcResult::Err(EcError::Response(EcResponseStatus::InvalidCommand)) => all_ok,
        // Other error or no EC
        _ => all_ok,
    }
}

/// Set (enable/disable UEFI App mode)
///
/// If error - ignore
/// If no EC - do nothing
/// Safe default.
///
/// The only issue could be if enabling works but disabling fails.
/// Then the system will be in a state where the power button is disabled, which is not good.
/// Only an EC reset will restore it.
pub fn set_uefi_app_mode(ec: &Option<CrosEc>, mode: bool) {
    let enable = if mode { 1 } else { 0 };
    if let Some(ref ec) = ec {
        // Ignore error
        // TODO: Is there anything we should do on error?
        let _res = EcRequestUefiAppMode { enable }.send_command(ec);
    }
}

pub fn keypress_mock_data(
    keypress: Option<Key>,
    input_deck: &mut InputDeckStatus,
    expansion_bay: &mut EcResponseExpansionBayStatus,
    // TODO: Also do something with this
    hw_diag: &mut EcResponseGetHwDiag,
) {
    // TODO: Can this be created from a macro?
    let zero_key = Char16::try_from('0').unwrap();
    let one_key = Char16::try_from('1').unwrap();
    let two_key = Char16::try_from('2').unwrap();
    let three_key = Char16::try_from('3').unwrap();
    let four_key = Char16::try_from('4').unwrap();
    let five_key = Char16::try_from('5').unwrap();
    let six_key = Char16::try_from('6').unwrap();
    let seven_key = Char16::try_from('7').unwrap();
    //let q_key = Char16::try_from('?').unwrap();
    let bay_present = EcResponseExpansionBayStatus {
        state: (ExpansionByStates::ModuleEnabled as u8)
            | (ExpansionByStates::HatchSwitchClosed as u8),
        board_id_0: 0,
        board_id_1: 0,
    };
    let diag_ok = EcResponseGetHwDiag {
        // 0 means no issues
        hw_diag: 0,
        // BIOS not complete, since it's a UEFI app
        bios_complete: 0,
    };

    match keypress {
        // State: NoExpansionBay
        Some(Key::Printable(key)) if key == zero_key => {
            *expansion_bay = EcResponseExpansionBayStatus {
                state: 0,
                board_id_0: 0,
                board_id_1: 0,
            };
            *hw_diag = EcResponseGetHwDiag {
                // All possible diags triggered
                hw_diag: 0xFF,
                // BIOS not complete, since it's a UEFI app
                bios_complete: 0,
            };
            *input_deck = InputDeckStatus {
                state: input_deck::InputDeckState::Disconnected,
                hubboard_present: false,
                touchpad_present: false,
                top_row: input_deck::TopRowPositions {
                    pos0: input_deck::InputModuleType::Disconnected,
                    pos1: input_deck::InputModuleType::Disconnected,
                    pos2: input_deck::InputModuleType::Disconnected,
                    pos3: input_deck::InputModuleType::Disconnected,
                    pos4: input_deck::InputModuleType::Disconnected,
                },
            };
        }
        _ => {}
    }
    match keypress {
        // State: NoHubboard
        Some(Key::Printable(key)) if key == one_key => {
            *expansion_bay = bay_present;
            *hw_diag = diag_ok;
            *input_deck = InputDeckStatus {
                state: input_deck::InputDeckState::Disconnected,
                hubboard_present: false,
                touchpad_present: false,
                top_row: input_deck::TopRowPositions {
                    pos0: input_deck::InputModuleType::Disconnected,
                    pos1: input_deck::InputModuleType::Disconnected,
                    pos2: input_deck::InputModuleType::Disconnected,
                    pos3: input_deck::InputModuleType::Disconnected,
                    pos4: input_deck::InputModuleType::Disconnected,
                },
            };
        }
        // State: TouchpadButNoInputmodules
        Some(Key::Printable(key)) if key == two_key => {
            *expansion_bay = bay_present;
            *hw_diag = diag_ok;
            *input_deck = InputDeckStatus {
                state: input_deck::InputDeckState::Disconnected,
                hubboard_present: true,
                touchpad_present: true,
                top_row: input_deck::TopRowPositions {
                    pos0: input_deck::InputModuleType::Disconnected,
                    pos1: input_deck::InputModuleType::Disconnected,
                    pos2: input_deck::InputModuleType::Disconnected,
                    pos3: input_deck::InputModuleType::Disconnected,
                    pos4: input_deck::InputModuleType::Disconnected,
                },
            };
        }
        // State: InputModulesMissing
        Some(Key::Printable(key)) if key == three_key => {
            *expansion_bay = bay_present;
            *hw_diag = diag_ok;
            *input_deck = InputDeckStatus {
                state: input_deck::InputDeckState::Disconnected,
                hubboard_present: true,
                touchpad_present: false,
                top_row: input_deck::TopRowPositions {
                    pos0: input_deck::InputModuleType::Disconnected,
                    pos1: input_deck::InputModuleType::Disconnected,
                    pos2: input_deck::InputModuleType::Disconnected,
                    pos3: input_deck::InputModuleType::Disconnected,
                    pos4: input_deck::InputModuleType::Disconnected,
                },
            };
        }
        // State: InputModulesMissing
        Some(Key::Printable(key)) if key == four_key => {
            *expansion_bay = bay_present;
            *hw_diag = diag_ok;
            *input_deck = InputDeckStatus {
                state: input_deck::InputDeckState::Disconnected,
                hubboard_present: true,
                touchpad_present: false,
                top_row: input_deck::TopRowPositions {
                    pos0: input_deck::InputModuleType::GenericC,
                    pos1: input_deck::InputModuleType::Disconnected,
                    pos2: input_deck::InputModuleType::Disconnected,
                    pos3: input_deck::InputModuleType::Disconnected,
                    pos4: input_deck::InputModuleType::Disconnected,
                },
            };
        }
        // State: InputModulesMissing
        Some(Key::Printable(key)) if key == five_key => {
            *expansion_bay = bay_present;
            *hw_diag = diag_ok;
            *input_deck = InputDeckStatus {
                state: input_deck::InputDeckState::Disconnected,
                hubboard_present: true,
                touchpad_present: false,
                top_row: input_deck::TopRowPositions {
                    pos0: input_deck::InputModuleType::GenericC,
                    pos1: input_deck::InputModuleType::GenericC,
                    pos2: input_deck::InputModuleType::Disconnected,
                    pos3: input_deck::InputModuleType::Disconnected,
                    pos4: input_deck::InputModuleType::Disconnected,
                },
            };
        }
        // State: NoTouchpad
        Some(Key::Printable(key)) if key == six_key => {
            *expansion_bay = bay_present;
            *hw_diag = diag_ok;
            *input_deck = InputDeckStatus {
                state: input_deck::InputDeckState::Disconnected,
                hubboard_present: true,
                touchpad_present: false,
                top_row: input_deck::TopRowPositions {
                    pos0: input_deck::InputModuleType::GenericC,
                    pos1: input_deck::InputModuleType::GenericC,
                    pos2: input_deck::InputModuleType::KeyboardA,
                    pos3: input_deck::InputModuleType::Disconnected,
                    pos4: input_deck::InputModuleType::Disconnected,
                },
            };
        }
        // State: Finished
        Some(Key::Printable(key)) if key == seven_key => {
            *expansion_bay = bay_present;
            *hw_diag = diag_ok;
            *input_deck = InputDeckStatus {
                state: input_deck::InputDeckState::On,
                hubboard_present: true,
                touchpad_present: true,
                top_row: input_deck::TopRowPositions {
                    pos0: input_deck::InputModuleType::GenericC,
                    pos1: input_deck::InputModuleType::GenericC,
                    pos2: input_deck::InputModuleType::KeyboardA,
                    pos3: input_deck::InputModuleType::Disconnected,
                    pos4: input_deck::InputModuleType::Disconnected,
                },
            };
        }
        _ => {}
    }
}
