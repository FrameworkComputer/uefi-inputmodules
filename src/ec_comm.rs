#[cfg(feature = "12in")]
use crate::state_machine_12in::InputDeckStatus12In;
use alloc::format;
use alloc::string::ToString;
use framework_lib::chromium_ec::commands::{
    EcRequestExpansionBayStatus, EcRequestGetHwDiag, EcRequestUefiAppBtnStatus,
    EcRequestUefiAppMode, EcResponseExpansionBayStatus, EcResponseGetHwDiag, ExpansionByStates,
    BOARD_VERSION_12, BOARD_VERSION_13, BOARD_VERSION_15, DIAGNOSTICS_AUDIO_DAUGHTERBOARD,
    DIAGNOSTICS_HW_NO_BATTERY, DIAGNOSTICS_NOFAN, DIAGNOSTICS_NO_LEFT_FAN,
    DIAGNOSTICS_NO_RIGHT_FAN, DIAGNOSTICS_TOUCHPAD,
};
use framework_lib::chromium_ec::input_deck::{self, InputDeckStatus};
use framework_lib::chromium_ec::input_deck::{InputDeckState, InputModuleType, TopRowPositions};
use framework_lib::chromium_ec::{CrosEc, EcError, EcRequestRaw, EcResponseStatus, EcResult};
use framework_lib::power::{
    get_accel_data, get_als_reading, AccelData, BatteryInformation, LidAngle, PowerInfo,
    ReducedPowerInfo,
};
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
#[cfg(not(feature = "12in"))]
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

#[repr(u8)]
#[derive(Copy, Clone, Debug)]
enum Framework12Adc {
    MainboardBoard,
    PowerButtonBoard,
    Psys,
    AdapterCurrent,
    Touchpad,
    AudioBoard,
}

/*
 * PLATFORM_EC_ADC_RESOLUTION default 10 bit
 *
 * +------------------+-----------+--------------+---------+----------------------+
 * |  BOARD VERSION   |  voltage  |  main board  |   GPU   |     Input module     |
 * +------------------+-----------+--------------+---------+----------------------+
 * | BOARD_VERSION_0  |  0    mv  |    Unused    |         |       Reserved       |
 * | BOARD_VERSION_1  |  173  mv  |    Unused    |         |       Reserved       |
 * | BOARD_VERSION_2  |  300  mv  |    Unused    |         |       Reserved       |
 * | BOARD_VERSION_3  |  430  mv  |    Unused    |         |       Reserved       |
 * | BOARD_VERSION_4  |  588  mv  |    EVT1      |         |       Reserved       |
 * | BOARD_VERSION_5  |  783  mv  |    Unused    |         |       Reserved       |
 * | BOARD_VERSION_6  |  905  mv  |    Unused    |         |       Reserved       |
 * | BOARD_VERSION_7  |  1033 mv  |    DVT1      |         |       Reserved       |
 * | BOARD_VERSION_8  |  1320 mv  |    DVT2      |         |    Generic A size    |
 * | BOARD_VERSION_9  |  1500 mv  |    PVT       |         |    Generic B size    |
 * | BOARD_VERSION_10 |  1650 mv  |    MP        |         |    Generic C size    |
 * | BOARD_VERSION_11 |  1980 mv  |    Unused    | RID_0   |    10 Key B size     |
 * | BOARD_VERSION_12 |  2135 mv  |    Unused    | RID_0,1 |       Keyboard       |
 * | BOARD_VERSION_13 |  2500 mv  |    Unused    | RID_0   |       Touchpad       |
 * | BOARD_VERSION_14 |  2706 mv  |    Unused    |         |       Reserved       |
 * | BOARD_VERSION_15 |  2813 mv  |    Unused    |         |    Not installed     |
 * +------------------+-----------+--------------+---------+----------------------+
 */

const BOARD_VERSION_COUNT: usize = 16;
const BOARD_VERSION: [i32; BOARD_VERSION_COUNT] = [
    85, 233, 360, 492, 649, 844, 965, 1094, 1380, 1562, 1710, 2040, 2197, 2557, 2766, 2814,
];

fn read_board_id(ec: &CrosEc, channel: Framework12Adc) -> EcResult<Option<u8>> {
    let mv = ec.adc_read(channel as u8)?;
    if mv < 0 {
        return Err(EcError::DeviceError(format!(
            "Failed to read ADC channel {}",
            channel as u8
        )));
    }

    for board_id in 0..BOARD_VERSION_COUNT {
        if mv < BOARD_VERSION[board_id] {
            return Ok(if board_id == 15 {
                None
            } else {
                Some(board_id as u8)
            });
        }
    }

    Err(EcError::DeviceError(format!(
        "Unknown board id. ADC mv: {}",
        mv
    )))
}

/// Get current state of hubboard and input deck
///
/// If no EC => unpopulated
/// If any error => populated
/// This will make sure the system can always boot,
/// but on testing systems (QEMU, 13inch) it will show an issue to help debug.
/// TODO: Implement this
#[cfg(feature = "12in")]
pub fn get_input_deck(ec: &Option<CrosEc>) -> InputDeckStatus12In {
    let unpopulated = InputDeckStatus12In {
        chassis_closed: false,
        pwrbtn_present: false,
        audioboard_present: false,
        touchpad_present: false,
    };

    // Pretend there's no EC because the EC is guaranteed not to have this command
    #[cfg(feature = "azalea")]
    let ec = None;

    if let Some(ref ec) = ec {
        let input_deck: core::result::Result<InputDeckStatus12In, ()> = {
            let intrusion = ec.get_intrusion_status().unwrap();
            let pwrbtn = read_board_id(ec, Framework12Adc::PowerButtonBoard).unwrap();
            let audio = read_board_id(ec, Framework12Adc::AudioBoard).unwrap();
            let tp = read_board_id(ec, Framework12Adc::Touchpad).unwrap();

            return InputDeckStatus12In {
                chassis_closed: !intrusion.currently_open,
                pwrbtn_present: pwrbtn.is_some(),
                audioboard_present: audio.is_some(),
                touchpad_present: tp.is_some(),
            };
        };
    }

    unpopulated
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

#[cfg(not(feature = "12in"))]
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
    let _seven_key = Char16::try_from('7').unwrap();
    let q_key = Char16::try_from('q').unwrap();
    let w_key = Char16::try_from('w').unwrap();
    let e_key = Char16::try_from('e').unwrap();
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
                // UMA Fan
                board_id_0: BOARD_VERSION_13,
                board_id_1: BOARD_VERSION_15,
            };
        }
        Some(Key::Printable(key)) if key == e_key => {
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

// TODO
#[cfg(feature = "12in")]
pub fn keypress_mock_data(
    keypress: Option<Key>,
    input_deck: &mut InputDeckStatus12In,
    // TODO: Also do something with this
    hw_diag: &mut EcResponseGetHwDiag,
    power_info: &mut Option<ReducedPowerInfo>,
) {
    // TODO: Can this be created from a macro?

    // Input deck control
    let one_key = Char16::try_from('1').unwrap();
    let two_key = Char16::try_from('2').unwrap();
    let three_key = Char16::try_from('3').unwrap();
    let four_key = Char16::try_from('4').unwrap();
    let five_key = Char16::try_from('5').unwrap();
    // AC/Battery power control
    let a_key = Char16::try_from('a').unwrap();
    let s_key = Char16::try_from('s').unwrap();
    let d_key = Char16::try_from('d').unwrap();
    let f_key = Char16::try_from('f').unwrap();
    let g_key = Char16::try_from('g').unwrap();
    // Fan (Cooler)
    let c_key = Char16::try_from('c').unwrap();
    // Audio board (Jack)
    let j_key = Char16::try_from('j').unwrap();
    // Power Button board
    let p_key = Char16::try_from('p').unwrap();

    match keypress {
        Some(Key::Printable(key)) if key == a_key => {
            *power_info = if let Some(mut info) = (*power_info).take() {
                info.ac_present = !info.ac_present;
                Some(info)
            } else {
                power_info.clone()
            };
        }
        Some(Key::Printable(key)) if key == c_key => {
            hw_diag.hw_diag ^= 1 << DIAGNOSTICS_NOFAN;
        }
        Some(Key::Printable(key)) if key == j_key => {
            hw_diag.hw_diag ^= 1 << DIAGNOSTICS_AUDIO_DAUGHTERBOARD;
        }
        _ => {}
    }
    match keypress {
        // Chassis not closed, touchpad board ID not present
        Some(Key::Printable(key)) if key == one_key => {
            hw_diag.hw_diag |= 1 << DIAGNOSTICS_TOUCHPAD;
            *input_deck = InputDeckStatus12In {
                chassis_closed: false,
                pwrbtn_present: input_deck.pwrbtn_present,
                audioboard_present: input_deck.audioboard_present,
                touchpad_present: false,
            };
        }
        // Chassis closed, touchpad board ID not present
        Some(Key::Printable(key)) if key == two_key => {
            *input_deck = InputDeckStatus12In {
                chassis_closed: true,
                pwrbtn_present: input_deck.pwrbtn_present,
                audioboard_present: input_deck.audioboard_present,
                touchpad_present: false,
            };
        }
        // Chassis closed, touchpad board ID present
        Some(Key::Printable(key)) if key == five_key => {
            *input_deck = InputDeckStatus12In {
                chassis_closed: true,
                pwrbtn_present: input_deck.pwrbtn_present,
                audioboard_present: input_deck.audioboard_present,
                touchpad_present: true,
            };
        }
        Some(Key::Printable(key)) if key == p_key => {
            *input_deck = InputDeckStatus12In {
                chassis_closed: input_deck.chassis_closed,
                pwrbtn_present: !input_deck.pwrbtn_present,
                audioboard_present: input_deck.audioboard_present,
                touchpad_present: input_deck.touchpad_present,
            };
        }
        Some(Key::Printable(key)) if key == j_key => {
            *input_deck = InputDeckStatus12In {
                chassis_closed: input_deck.chassis_closed,
                pwrbtn_present: input_deck.pwrbtn_present,
                audioboard_present: !input_deck.audioboard_present,
                touchpad_present: input_deck.touchpad_present,
            };
        }
        Some(Key::Printable(key)) if key == a_key => {
            hw_diag.hw_diag |= 1 << DIAGNOSTICS_HW_NO_BATTERY;
            *power_info = Some(
                PowerInfo {
                    ac_present: true,
                    battery: None,
                }
                .into(),
            )
        }
        Some(Key::Printable(key)) if key == s_key => {
            hw_diag.hw_diag &= !(1 << DIAGNOSTICS_HW_NO_BATTERY);
            *power_info = Some(
                PowerInfo {
                    ac_present: false,
                    // TODO: Adjust battery information for 51Wh battery
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
            hw_diag.hw_diag &= !(1 << DIAGNOSTICS_HW_NO_BATTERY);
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
            hw_diag.hw_diag &= !(1 << DIAGNOSTICS_HW_NO_BATTERY);
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
            hw_diag.hw_diag &= !(1 << DIAGNOSTICS_HW_NO_BATTERY);
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

pub fn get_als_data(ec: &Option<CrosEc>) -> u32 {
    let ec = if let Some(ref ec) = ec {
        ec
    } else {
        return 0;
    };

    if let Some(reading) = get_als_reading(ec) {
        reading
    } else {
        0
    }
}

pub fn get_accel_reading(ec: &Option<CrosEc>) -> (AccelData, AccelData, LidAngle) {
    if let Some(ref ec) = ec {
        get_accel_data(ec)
    } else {
        (
            AccelData { x: 0, y: 0, z: 0 },
            AccelData { x: 0, y: 0, z: 0 },
            LidAngle::Unreliable,
        )
    }
}
