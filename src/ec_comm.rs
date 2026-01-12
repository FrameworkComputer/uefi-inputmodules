use alloc::string::{String, ToString};
use framework_lib::chromium_ec::commands::{
    EcRequestExpansionBayStatus, EcRequestGetHwDiag, EcRequestUefiAppBtnStatus,
    EcRequestUefiAppMode, EcResponseExpansionBayStatus, EcResponseGetHwDiag, ExpansionByStates,
    BOARD_VERSION_12, BOARD_VERSION_13, BOARD_VERSION_15, DIAGNOSTICS_NO_LEFT_FAN,
    DIAGNOSTICS_NO_RIGHT_FAN,
};
use framework_lib::chromium_ec::input_deck::{self, InputDeckStatus};
use framework_lib::chromium_ec::input_deck::{InputDeckState, InputModuleType, TopRowPositions};
use framework_lib::chromium_ec::{
    CrosEc, CrosEcDriver, EcError, EcRequestRaw, EcResponseStatus, EcResult,
};
use framework_lib::power::{BatteryInformation, PowerInfo, ReducedPowerInfo};
#[allow(unused_imports)]
use log::{debug, error, info, trace};
use uefi::proto::console::text::Key;
use uefi::Char16;

// TODO: Mark as public in framework-lib
const EC_MEMMAP_FAN: u16 = 0x10;
/// Used on old EC firmware (before 2023)
const EC_FAN_SPEED_STALLED_DEPRECATED: u16 = 0xFFFE;
const EC_FAN_SPEED_NOT_PRESENT: u16 = 0xFFFF;

pub fn version_info(ec: &Option<CrosEc>) -> String {
    if let Some(ec) = ec {
        if let Ok(version) = ec.version_info() {
            version.to_string()
        } else {
            "???".to_string()
        }
    } else {
        "???".to_string()
    }
}

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
            pos0: InputModuleType::GenericB,
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
        hw_diag: 0xFFFFFFFF,
        // BIOS not complete, since it's a UEFI app
        bios_complete: 0,
        device_complete: 0,
    };
    let all_ok = EcResponseGetHwDiag {
        // No HW diag issue
        hw_diag: 0,
        // BIOS not complete, since it's a UEFI app
        bios_complete: 0,
        device_complete: 1,
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

pub fn get_fan_speed(ec: &Option<CrosEc>) -> (u16, u16) {
    let mut speed = (0, 0);

    // Pretend there's no EC because the EC is guaranteed not to have this command
    #[cfg(feature = "azalea")]
    let ec = None;

    let ec = if let Some(ec) = ec {
        ec
    } else {
        // If no EC, just return 0
        return speed;
    };

    if let Some(fans) = ec.read_memory(EC_MEMMAP_FAN, 0x08) {
        for i in 0..2 {
            let fan = u16::from_le_bytes([fans[i * 2], fans[1 + i * 2]]);
            if fan == EC_FAN_SPEED_STALLED_DEPRECATED {
                info!("Fan {} stalled", i);
            } else if fan == EC_FAN_SPEED_NOT_PRESENT {
                info!("Fan {} not present", i);
            } else {
                if i == 0 {
                    speed.0 = fan;
                } else if i == 1 {
                    speed.1 = fan;
                }
            }
        }
    }
    speed
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
    power_info: &mut Option<ReducedPowerInfo>,
) {
    // TODO: Can this be created from a macro?
    let one_key = Char16::try_from('1').unwrap();
    let two_key = Char16::try_from('2').unwrap();
    let three_key = Char16::try_from('3').unwrap();
    let four_key = Char16::try_from('4').unwrap();
    let five_key = Char16::try_from('5').unwrap();
    let six_key = Char16::try_from('6').unwrap();
    let seven_key = Char16::try_from('7').unwrap();
    let q_key = Char16::try_from('q').unwrap();
    let w_key = Char16::try_from('w').unwrap();
    let e_key = Char16::try_from('e').unwrap();
    let r_key = Char16::try_from('r').unwrap();
    let z_key = Char16::try_from('z').unwrap();
    let x_key = Char16::try_from('x').unwrap();
    let c_key = Char16::try_from('c').unwrap();
    let v_key = Char16::try_from('v').unwrap();
    let a_key = Char16::try_from('a').unwrap();
    let s_key = Char16::try_from('s').unwrap();
    let d_key = Char16::try_from('d').unwrap();
    let f_key = Char16::try_from('f').unwrap();
    let g_key = Char16::try_from('g').unwrap();
    let l_key = Char16::try_from('l').unwrap();
    let h_key = Char16::try_from('h').unwrap();
    let b_key = Char16::try_from('b').unwrap();

    match keypress {
        Some(Key::Printable(key)) if key == a_key => {
            *power_info = if let Some(mut info) = (*power_info).take() {
                info.ac_present = !info.ac_present;
                Some(info)
            } else {
                power_info.clone()
            };
        }
        Some(Key::Printable(key)) if key == h_key => {
            input_deck.hubboard_present = !input_deck.hubboard_present;
        }
        Some(Key::Printable(key)) if key == l_key => {
            if (*expansion_bay).hatch_switch_closed() {
                expansion_bay.state &= !(ExpansionByStates::HatchSwitchClosed as u8);
            } else {
                expansion_bay.state |= ExpansionByStates::HatchSwitchClosed as u8;
            }
        }
        Some(Key::Printable(key)) if key == z_key => {
            hw_diag.hw_diag = (1 << DIAGNOSTICS_NO_LEFT_FAN) + (1 << DIAGNOSTICS_NO_RIGHT_FAN);
        }
        Some(Key::Printable(key)) if key == x_key => {
            hw_diag.hw_diag = 1 << DIAGNOSTICS_NO_RIGHT_FAN;
        }
        Some(Key::Printable(key)) if key == c_key => {
            hw_diag.hw_diag = 1 << DIAGNOSTICS_NO_LEFT_FAN;
        }
        Some(Key::Printable(key)) if key == v_key => {
            hw_diag.hw_diag = 0;
        }
        Some(Key::Printable(key)) if key == b_key => {
            hw_diag.device_complete = if hw_diag.device_complete == 0 { 1 } else { 0 };
        }
        Some(Key::Printable(key)) if key == q_key => {
            (*expansion_bay) = EcResponseExpansionBayStatus {
                state: if expansion_bay.hatch_switch_closed() {
                    ExpansionByStates::HatchSwitchClosed as u8
                } else {
                    0
                },
                // No module
                board_id_0: BOARD_VERSION_15,
                board_id_1: BOARD_VERSION_15,
            };
        }
        Some(Key::Printable(key)) if key == w_key => {
            (*expansion_bay) = EcResponseExpansionBayStatus {
                state: if expansion_bay.hatch_switch_closed() {
                    ExpansionByStates::HatchSwitchClosed as u8
                } else {
                    0
                },
                // Bad connection
                board_id_0: BOARD_VERSION_13,
                board_id_1: BOARD_VERSION_13,
            };
        }
        Some(Key::Printable(key)) if key == e_key => {
            (*expansion_bay) = EcResponseExpansionBayStatus {
                state: if expansion_bay.hatch_switch_closed() {
                    ExpansionByStates::HatchSwitchClosed as u8
                } else {
                    0
                },
                // UMA Fan
                board_id_0: BOARD_VERSION_13,
                board_id_1: BOARD_VERSION_15,
            };
        }
        Some(Key::Printable(key)) if key == r_key => {
            (*expansion_bay) = EcResponseExpansionBayStatus {
                state: if expansion_bay.hatch_switch_closed() {
                    ExpansionByStates::HatchSwitchClosed as u8
                } else {
                    0
                },
                // Dual Interposer
                board_id_0: BOARD_VERSION_12,
                board_id_1: BOARD_VERSION_12,
            };
        }
        _ => {}
    }
    match keypress {
        // State: TouchpadButNoInputmodules
        Some(Key::Printable(key)) if key == one_key => {
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
        Some(Key::Printable(key)) if key == two_key => {
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
        Some(Key::Printable(key)) if key == three_key => {
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
        Some(Key::Printable(key)) if key == four_key => {
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
        Some(Key::Printable(key)) if key == five_key => {
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
        Some(Key::Printable(key)) if key == six_key => {
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
        // State: Finished
        Some(Key::Printable(key)) if key == seven_key => {
            *input_deck = InputDeckStatus {
                state: input_deck::InputDeckState::On,
                hubboard_present: true,
                touchpad_present: true,
                top_row: input_deck::TopRowPositions {
                    pos0: input_deck::InputModuleType::FullWidth,
                    pos1: input_deck::InputModuleType::Disconnected,
                    pos2: input_deck::InputModuleType::Disconnected,
                    pos3: input_deck::InputModuleType::Disconnected,
                    pos4: input_deck::InputModuleType::Disconnected,
                },
            };
        }
        Some(Key::Printable(key)) if key == a_key => {
            *power_info = Some(
                PowerInfo {
                    ac_present: true,
                    battery: None,
                }
                .into(),
            )
        }
        Some(Key::Printable(key)) if key == s_key => {
            *power_info = Some(
                PowerInfo {
                    ac_present: false,
                    battery: Some(BatteryInformation {
                        present_voltage: 16280,  // 16.280
                        present_rate: 674,       // 674mA
                        remaining_capacity: 293, // 293mAh, 4.339Wh
                        battery_count: 0,
                        current_battery_index: 0,
                        design_capacity: 55800,          // 3572mAh, 55.8Wh
                        design_voltage: 15400,           // 15.400V
                        last_full_charge_capacity: 3154, // 3154mAh
                        cycle_count: 320,
                        charge_percentage: 5,
                        manufacturer: "NVT".to_string(),
                        model_number: "Framewo".to_string(),
                        serial_number: "027B".to_string(),
                        battery_type: "LION".to_string(),
                        discharging: true,
                        charging: false,
                        level_critical: false,
                    }),
                }
                .into(),
            )
        }
        Some(Key::Printable(key)) if key == d_key => {
            *power_info = Some(
                PowerInfo {
                    ac_present: true,
                    battery: Some(BatteryInformation {
                        present_voltage: 16280,  // 16.280
                        present_rate: 2348,      // 2348mA
                        remaining_capacity: 180, // 180mAh, 2.930Wh
                        battery_count: 0,
                        current_battery_index: 0,
                        design_capacity: 55800,          // 3572mAh, 55.8Wh
                        design_voltage: 15400,           // 15.400V
                        last_full_charge_capacity: 3154, // 3154mAh
                        cycle_count: 320,
                        charge_percentage: 5,
                        manufacturer: "NVT".to_string(),
                        model_number: "Framewo".to_string(),
                        serial_number: "027B".to_string(),
                        battery_type: "LION".to_string(),
                        discharging: false,
                        charging: true,
                        level_critical: false,
                    }),
                }
                .into(),
            )
        }
        Some(Key::Printable(key)) if key == f_key => {
            *power_info = Some(
                PowerInfo {
                    ac_present: false,
                    battery: Some(BatteryInformation {
                        present_voltage: 15412,   // 15.412
                        present_rate: 552,        // 552mA
                        remaining_capacity: 1939, // 1939mAh, 29.883Wh
                        battery_count: 0,
                        current_battery_index: 0,
                        design_capacity: 55800,          // 3572mAh, 55.8Wh
                        design_voltage: 15400,           // 15.400V
                        last_full_charge_capacity: 3154, // 3154mAh
                        cycle_count: 320,
                        charge_percentage: 61,
                        manufacturer: "NVT".to_string(),
                        model_number: "Framewo".to_string(),
                        serial_number: "027B".to_string(),
                        battery_type: "LION".to_string(),
                        discharging: true,
                        charging: false,
                        level_critical: false,
                    }),
                }
                .into(),
            )
        }
        Some(Key::Printable(key)) if key == g_key => {
            *power_info = Some(
                PowerInfo {
                    ac_present: true,
                    battery: Some(BatteryInformation {
                        present_voltage: 17135,   // 17.135V
                        present_rate: 2460,       // 2460mA
                        remaining_capacity: 1939, // 1939mAh, 29.883Wh
                        battery_count: 0,
                        current_battery_index: 0,
                        design_capacity: 55800,          // 3572mAh, 55.8Wh
                        design_voltage: 15400,           // 15.400V
                        last_full_charge_capacity: 3154, // 3154mAh
                        cycle_count: 320,
                        charge_percentage: 61,
                        manufacturer: "NVT".to_string(),
                        model_number: "Framewo".to_string(),
                        serial_number: "027B".to_string(),
                        battery_type: "LION".to_string(),
                        discharging: false,
                        charging: true,
                        level_critical: false,
                    }),
                }
                .into(),
            )
        }
        _ => {}
    }
}
