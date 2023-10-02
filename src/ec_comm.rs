use alloc::string::ToString;
use framework_lib::chromium_ec::commands::{
    EcRequestExpansionBayStatus, EcRequestGetHwDiag, EcRequestUefiAppBtnStatus,
    EcRequestUefiAppMode, EcResponseExpansionBayStatus, EcResponseGetHwDiag, ExpansionByStates,
};
use framework_lib::chromium_ec::input_deck::{self, InputDeckStatus};
use framework_lib::chromium_ec::input_deck::{InputDeckState, InputModuleType, TopRowPositions};
use framework_lib::chromium_ec::{CrosEc, EcError, EcRequestRaw, EcResponseStatus, EcResult};
#[allow(unused_imports)]
use log::{debug, error, info, trace};
use uefi::proto::console::text::Key;
use uefi::Char16;

pub fn get_expansion_bay(ec: &Option<CrosEc>) -> EcResponseExpansionBayStatus {
    let emulated_bay = EcResponseExpansionBayStatus {
        state: 0,
        board_id_0: 0,
        board_id_1: 0,
    };

    let ec = ec.as_ref().ok_or(EcError::DeviceError("No EC".to_string()));
    #[cfg(not(feature = "azalea"))]
    let expansion_bay = ec.and_then(|ec| EcRequestExpansionBayStatus {}.send_command(ec));
    #[cfg(feature = "azalea")]
    let expansion_bay: Result<EcResponseExpansionBayStatus, ()> = Result::Err(());
    if let Ok(expansion_bay) = expansion_bay {
        //println!("Got it!");
        expansion_bay
    } else {
        //println!("Emulated bay data");
        emulated_bay
    }
}

pub fn get_input_deck(ec: &Option<CrosEc>) -> InputDeckStatus {
    let emulated_deck = InputDeckStatus {
        state: InputDeckState::On,
        hubboard_present: false,
        touchpad_present: true,
        top_row: TopRowPositions {
            pos0: InputModuleType::GenericC,
            pos1: InputModuleType::GenericC,
            pos2: InputModuleType::GenericA,
            pos3: InputModuleType::Disconnected,
            pos4: InputModuleType::Disconnected,
        },
    };

    let ec = ec.as_ref().ok_or(EcError::DeviceError("No EC".to_string()));
    #[cfg(not(feature = "azalea"))]
    let input_deck = ec.and_then(|ec| ec.get_input_deck_status());
    #[cfg(feature = "azalea")]
    let input_deck: Result<InputDeckStatus, ()> = Result::Err(());
    if let Ok(input_deck) = input_deck {
        //println!("Got it!");
        input_deck
    } else {
        //println!("Emulated deck data");
        emulated_deck
    }
}

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

pub fn get_hw_diag(ec: &Option<CrosEc>) -> EcResponseGetHwDiag {
    let emulated_diag = EcResponseGetHwDiag {
        // 0 means no issues
        //hw_diag: 0,
        hw_diag: 0xFF,
        // BIOS not complete, since it's a UEFI app
        bios_complete: 0,
    };

    let ec = ec.as_ref().ok_or(EcError::DeviceError("No EC".to_string()));
    #[cfg(not(feature = "azalea"))]
    let hw_diag = ec.and_then(|ec| EcRequestGetHwDiag {}.send_command(ec));
    #[cfg(feature = "azalea")]
    let hw_diag: Result<EcResponseGetHwDiag, ()> = Result::Err(());

    match hw_diag {
        Ok(hw_diag) => hw_diag,
        // On old EC version
        EcResult::Err(EcError::Response(EcResponseStatus::InvalidCommand)) => emulated_diag,
        // Other error
        _ => emulated_diag,
    }
}

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
