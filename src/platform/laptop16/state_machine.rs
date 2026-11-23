// TODO: For when I ported the font
// use alloc::string::ToString;
use crate::alloc::string::ToString;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use embedded_graphics::geometry::Size;
use embedded_graphics::prelude::Point;

use dmidecode::Structure;
use framework_lib::ccgx::{AppVersion, Application, MainPdVersions};
use framework_lib::chromium_ec::commands::{
    EcResponseExpansionBayStatus, EcResponseGetHwDiag, ExpansionBayBoard, ExpansionBayIssue,
};
use framework_lib::chromium_ec::input_deck::{InputDeckStatus, InputModuleType};
use framework_lib::chromium_ec::CrosEc;
use framework_lib::power::{self, ReducedPowerInfo};
#[cfg(not(feature = "qemu"))]
use framework_lib::smbios::get_smbios;
#[cfg(feature = "qemu")]
use framework_lib::smbios::SmbiosStore;
use framework_lib::smbios::{get_platform, Platform as SmbiosPlatform};
#[allow(unused_imports)]
use log::{debug, error, info, trace};
use uefi::proto::console::text::Key;
use uefi::Char16;
#[allow(unused_imports)]
use uefi::{print, println};

use hwio::{Io, Pio};

use super::drawing::*;
use super::{ec, status};
use crate::buffer::Buffer;
use crate::ec_comm;
use crate::platform::{LoopInfo, Platform, Tick};
use crate::uefi_usb::{self, UsbDevice};
use crate::ui::{draw_battery_info, draw_textbox, TextColor};

const BOOT_TIME_SECONDS: u32 = 20;

pub const LOTUS_PD1_VER: AppVersion = AppVersion {
    application: Application::Notebook,
    major: 0,
    minor: 0,
    circuit: 0x21,
};

pub const LOTUS_PD2_VER: AppVersion = AppVersion {
    application: Application::Notebook,
    major: 0,
    minor: 0,
    circuit: 0x21,
};

pub const TULIP_PD1_VER: AppVersion = AppVersion {
    application: Application::Notebook,
    major: 0,
    minor: 0,
    circuit: 0x23,
};

pub const TULIP_PD2_VER: AppVersion = AppVersion {
    application: Application::Notebook,
    major: 0,
    minor: 0,
    circuit: 0x23,
};

pub const PD3_VER: AppVersion = AppVersion {
    application: Application::Notebook,
    major: 0,
    minor: 0,
    circuit: 0x23,
};

#[allow(dead_code)]
pub struct DebugInfo {
    pub bios_version: String,
    pub ec_version: String,
    pub power_button: bool,
    pub ec_present: bool,
    pub long_press: i32,
    pub bay_state: u8,
    pub bay_module: Result<ExpansionBayBoard, ExpansionBayIssue>,
    pub gpu0: u8,
    pub gpu1: u8,
    pub bay_enabled: bool,
    pub bay_fault: bool,
    pub hatch_closed: bool,
    pub hw_diag: EcResponseGetHwDiag,
    pub fan_speed: (u16, u16),
    pub deck: InputDeckStatus,
    pub power_info: Option<ReducedPowerInfo>,
    pub pd1_ver: Option<AppVersion>,
    pub pd2_ver: Option<AppVersion>,
    pub pd3_ver: Option<AppVersion>,
    pub usb_devices: Vec<UsbDevice>,
}

/// Hardware state, read once per loop iteration
#[derive(Clone)]
pub struct Snapshot {
    input_deck: InputDeckStatus,
    expansion_bay: EcResponseExpansionBayStatus,
    hw_diag: EcResponseGetHwDiag,
    fan_speed: (u16, u16),
    power_info: Option<ReducedPowerInfo>,
    usb_devices: Vec<UsbDevice>,
}

pub struct State {
    // Cache bitmaps for drawing
    bitmaps: StateBitmaps,
    screen_size: Size,
    // Currently displayed frame
    frame: StateFrame,
    bios_version: String,
    ec_version: String,
    pd1_ver: Option<AppVersion>,
    pd2_ver: Option<AppVersion>,
    pd3_ver: Option<AppVersion>,
    outdated_firmware: OutdatedFirmware,
    mock_outdated_fw: bool,
    // Record the time when the warning shows up and the timer starts
    // So that the countdown does not start when the user is still installing inputmodules
    warning_start_time: Option<u32>,
    last_second: u32,
    countdown_text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BayIssue {
    LeftFan,
    RightFan,
    BothFans,
    InterposerOpen,
    ModuleMissing,
    BadConnection,
}
impl fmt::Display for BayIssue {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            BayIssue::LeftFan => write!(f, "Left Fan not detected"),
            BayIssue::RightFan => write!(f, "Right Fan not detected"),
            BayIssue::BothFans => write!(f, "Both Fans not detected"),
            BayIssue::InterposerOpen => write!(f, "Interposer Door not closed"),
            BayIssue::ModuleMissing => write!(f, "Expansion Bay Module not detected"),
            BayIssue::BadConnection => write!(f, "Expansion Bay Cable improperly connected"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
struct OutdatedFirmware {
    keyboard: Vec<String>,
    pd1_controller: Option<String>,
    pd2_controller: Option<String>,
    pd3_controller: Option<String>,
}
impl OutdatedFirmware {
    fn is_empty(&self) -> bool {
        self.keyboard.is_empty()
            && self.pd1_controller.is_none()
            && self.pd2_controller.is_none()
            && self.pd3_controller.is_none()
    }
    fn merge_with(&mut self, other: &Self) {
        self.keyboard.extend(other.keyboard.clone());
        if other.pd1_controller.is_some() {
            self.pd1_controller = other.pd1_controller.clone();
        }
        if other.pd2_controller.is_some() {
            self.pd2_controller = other.pd2_controller.clone();
        }
        if other.pd3_controller.is_some() {
            self.pd3_controller = other.pd3_controller.clone();
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum StateFrame {
    NoExpansionBay(BayIssue),
    /// The interposer door was closed
    /// Meaning the Expansion Bay Module was powered on after BIOS early init, need to reboot
    InterposerClosed,
    NoHubboard,
    TouchpadButNoInputmodules,
    InputModulesMissing,
    NoTouchpad,
    OutdatedFirmware(OutdatedFirmware),
    /// EC diagnostics still in progress (device_complete == 0)
    CheckingDiagnostics,
    Finished,
}

impl State {
    fn new(ec: &Option<CrosEc>) -> Self {
        #[cfg(feature = "qemu")]
        let smbios: Option<SmbiosStore> = None;
        #[cfg(not(feature = "qemu"))]
        let smbios = get_smbios();
        let bios_version = smbios
            .as_ref()
            .and_then(|smbios| {
                smbios.structures().find_map(|r| match r {
                    Ok(Structure::Bios(bios)) => Some(bios.bios_version.to_string()),
                    _ => None,
                })
            })
            .unwrap_or_else(|| "?.??".to_string());
        let ec_version = ec_comm::version_info(ec);

        let mut outdated_firmware = OutdatedFirmware::default();
        let mut pd1_ver = None;
        let mut pd2_ver = None;
        let mut pd3_ver = None;
        let versions = ec.as_ref().and_then(|x| power::read_pd_version(x).ok());
        if let Some(MainPdVersions::Many(versions)) = versions {
            if let Some(SmbiosPlatform::Framework16Amd7080) = get_platform() {
                if let Some(ver) = versions.get(0) {
                    pd1_ver = Some(ver.app);
                    if ver.app != LOTUS_PD1_VER {
                        outdated_firmware.pd1_controller = Some(format!(
                            "Please manually reinstall the BIOS to update PD1 firmware. Currently: {}. Expected: {}.",
                            ver.app, LOTUS_PD1_VER
                        ));
                    }
                }
                if let Some(ver) = versions.get(1) {
                    pd2_ver = Some(ver.app);
                    if ver.app != LOTUS_PD2_VER {
                        outdated_firmware.pd2_controller = Some(format!(
                            "Please manually reinstall the BIOS to update PD2 firmware. Currently: {}. Expected: {}.",
                            ver.app, LOTUS_PD2_VER
                        ));
                    }
                }
            } else {
                if let Some(ver) = versions.get(0) {
                    pd1_ver = Some(ver.app);
                    if ver.app != TULIP_PD1_VER {
                        outdated_firmware.pd1_controller = Some(format!(
                            "Please manually reinstall the BIOS to update PD1 firmware. Currently: {}. Expected: {}.",
                            ver.app, TULIP_PD1_VER
                        ));
                    }
                }
                if let Some(ver) = versions.get(1) {
                    pd2_ver = Some(ver.app);
                    if ver.app != TULIP_PD2_VER {
                        outdated_firmware.pd2_controller = Some(format!(
                            "Please manually reinstall the BIOS to update PD2 firmware. Currently: {}. Expected: {}.",
                            ver.app, TULIP_PD2_VER
                        ));
                    }
                }
            }
            if let Some(ver) = versions.get(2) {
                pd3_ver = Some(ver.app);
                if ver.app != PD3_VER {
                    outdated_firmware.pd3_controller = Some(format!(
                        "Please manually reinstall the BIOS to update dGPU firmware. Currently: {}. Expected: {}.",
                        ver.app, PD3_VER
                    ));
                }
            }
        }
        #[cfg(feature = "qemu")]
        {
            outdated_firmware.pd1_controller = Some(format!(
                "Please manually reinstall the BIOS to update PD1 firmware. Currently: ??.??.??. Expected: {}.",
                LOTUS_PD1_VER
            ));
            outdated_firmware.pd2_controller = Some(format!(
                "Please manually reinstall the BIOS to update PD2 firmware. Currently: ??.??.??. Expected: {}.",
                LOTUS_PD2_VER
            ));
            outdated_firmware.pd3_controller = Some(format!(
                "Please manually reinstall the BIOS to update dGPU firmware. Currently: ??.??.??. Expected: {}.",
                PD3_VER
            ));
        }

        Self {
            bitmaps: StateBitmaps::default(),
            screen_size: Size {
                width: WIDTH as u32,
                height: HEIGHT as u32,
            },
            // Doesn't matter where we start from, will be updated immediately, before checking
            frame: StateFrame::Finished,
            bios_version,
            ec_version,
            pd1_ver,
            pd2_ver,
            pd3_ver,
            outdated_firmware,
            mock_outdated_fw: false,
            warning_start_time: None,
            last_second: 0,
            countdown_text: String::new(),
        }
    }

    // Very simple state machine
    // The next state does not depend on the current state
    fn next_state(&self, prev: Option<&Snapshot>, cur: &Snapshot) -> StateFrame {
        let Snapshot {
            input_deck,
            expansion_bay,
            hw_diag,
            usb_devices,
            ..
        } = cur;

        // Can't get out of this, need to reboot
        if self.frame == StateFrame::InterposerClosed {
            return self.frame.clone();
        }

        let prev_bay_issue =
            prev.and_then(|prev| has_bay_issue(&prev.expansion_bay, &prev.hw_diag));
        let bay_issue = has_bay_issue(expansion_bay, hw_diag);
        let door_was_open = Some(BayIssue::InterposerOpen) == prev_bay_issue;
        let door_now_open = Some(BayIssue::InterposerOpen) == bay_issue;
        let need_reinit = matches!(
            expansion_bay.expansion_bay_board(),
            Ok(ExpansionBayBoard::DualInterposer) | Ok(ExpansionBayBoard::SingleInterposer)
        );
        // If the user closes the door, we need to reboot to reinitialize the module
        // But only if it's a GPU or similar module
        if door_was_open && !door_now_open && need_reinit {
            return StateFrame::InterposerClosed;
        }

        // Expected only as the first state
        // Otherwise won't usually happen because you're not supposed to hot-plug the module.
        // Can still happen as later state if the user opens the lid or hatch.
        if let Some(issue) = bay_issue {
            return StateFrame::NoExpansionBay(issue);
        }

        if !input_deck.hubboard_present {
            return StateFrame::NoHubboard;
        }

        match (
            input_deck.touchpad_present,
            input_deck.top_row_fully_populated(),
        ) {
            (false, false) => StateFrame::InputModulesMissing,
            (false, true) => StateFrame::NoTouchpad,
            (true, false) => StateFrame::TouchpadButNoInputmodules,
            (true, true) => {
                let mut outdated_fw_msgs = OutdatedFirmware::default();

                // In QEMU force an error message
                #[cfg(feature = "qemu")]
                if self.mock_outdated_fw {
                    outdated_fw_msgs.keyboard.push(format!(
                        "Firmware of {} is outdated. Please update to v0.3.0 or later.",
                        "Framework 16 - ANSI Keyboard"
                    ));
                }

                for dev in usb_devices {
                    // Show a warning of older than 0.3.1
                    if dev.is_framework_keyboard() && dev.bcd_device < 0x0031 {
                        let name = dev.get_name().unwrap_or("???").trim();
                        outdated_fw_msgs.keyboard.push(format!(
                            "Firmware of {} is outdated. Please update to v0.3.1 or later.",
                            name
                        ));
                    }
                }

                if !cfg!(feature = "qemu") || self.mock_outdated_fw {
                    outdated_fw_msgs.merge_with(&self.outdated_firmware);
                }

                if !outdated_fw_msgs.is_empty() {
                    StateFrame::OutdatedFirmware(outdated_fw_msgs)
                } else if hw_diag.device_complete == 0 {
                    StateFrame::CheckingDiagnostics
                } else {
                    StateFrame::Finished
                }
            }
        }
    }

    // Display graphics for the current frame
    // Need to call blit after the function call to show it
    pub fn draw_state(&mut self, buffer: &mut Buffer, input_deck: &InputDeckStatus) {
        // Draw solid background color #F3F2F2
        for y in 0..self.screen_size.height {
            for x in 0..self.screen_size.width {
                // TODO: Can actually unwrap because the screen size should be correct.
                if let Some(pixel) = buffer.pixel(x as usize, y as usize) {
                    pixel.red = 0xF3;
                    pixel.green = 0xF2;
                    pixel.blue = 0xF2;
                }
            }
        }

        // Draw dark grey bar at the bottom
        draw_black_bar(buffer);

        // Draw laptop with latches open
        self.bitmaps.laptop_latches_open().draw(buffer);

        // Fix up the laptop
        match self.frame {
            StateFrame::NoExpansionBay(BayIssue::InterposerOpen) | StateFrame::InterposerClosed => {
                self.bitmaps.latch_warning().draw(buffer)
            }
            StateFrame::NoExpansionBay(_) => self.bitmaps.expansion_bay_missing().draw(buffer),
            StateFrame::NoHubboard => self.bitmaps.hubboard_missing().draw(buffer),
            _ => {}
        }

        // Hints on lower left and right
        match self.frame {
            StateFrame::Finished => {
                // Boot Button indicator right
                self.bitmaps.button_boot().draw(buffer);

                // Overlay latches closed
                self.bitmaps.latches_closed_left().draw(buffer);
                self.bitmaps.latches_closed_right().draw(buffer);
            }
            StateFrame::OutdatedFirmware(_) => {
                self.bitmaps.button_short_skip().draw(buffer);
                self.bitmaps.button_long_off().draw(buffer);

                // Overlay latches closed
                self.bitmaps.latches_closed_left().draw(buffer);
                self.bitmaps.latches_closed_right().draw(buffer);
            }
            StateFrame::InterposerClosed => {
                self.bitmaps.button_short_reboot().draw(buffer);
                self.bitmaps.button_long_off().draw(buffer);
            }
            _ => {
                self.bitmaps.button_short_skip().draw(buffer);
                self.bitmaps.button_long_off().draw(buffer);
            }
        }

        // QR Codes
        match &self.frame {
            StateFrame::NoExpansionBay(_) | StateFrame::InterposerClosed => {
                self.bitmaps.qr_code_1().draw(buffer);
            }
            StateFrame::NoHubboard => {
                self.bitmaps.qr_code_2().draw(buffer);
            }
            StateFrame::Finished | StateFrame::CheckingDiagnostics => {}
            StateFrame::OutdatedFirmware(msgs) => {
                let side_pd_version_issue =
                    msgs.pd1_controller.is_some() || msgs.pd2_controller.is_some();
                match (
                    !msgs.keyboard.is_empty(),
                    side_pd_version_issue,
                    msgs.pd3_controller.is_some(),
                ) {
                    // No issue, no QR code needed
                    (false, false, false) => {}
                    (false, false, true) => {
                        self.bitmaps.qr_code_pd().draw(buffer);
                    }
                    (false, true, false) => {
                        self.bitmaps.qr_code_side_pd().draw(buffer);
                    }
                    // If dGPU and side PD both are outdated, show only the side PD QR code for two reasons.
                    // 1. They both redirect to the same KB article - no need to duplicate.
                    // 2. Side PD not in sync with BIOS version means there was a bug during the BIOS update.
                    // We use the side PD QR code to track who experienced the updater bug, so prioritize that QR over the other one
                    (false, true, true) => {
                        self.bitmaps.qr_code_side_pd().draw(buffer);
                    }
                    (true, false, false) => {
                        self.bitmaps.qr_code_kb().draw(buffer);
                    }
                    (true, false, true) => {
                        self.bitmaps.qr_code_kb().draw(buffer);
                        // To show both QR codes at the same time we need to move one up a bit
                        // By the height (198) and some extra padding
                        self.bitmaps.qr_code_pd().draw_pos(
                            buffer,
                            Point {
                                x: 1814,
                                y: 967 - 198 - 15,
                            },
                        );
                    }
                    (true, true, false) => {
                        self.bitmaps.qr_code_kb().draw(buffer);
                        // To show both QR codes at the same time we need to move one up a bit
                        // By the height (198) and some extra padding
                        self.bitmaps.qr_code_side_pd().draw_pos(
                            buffer,
                            Point {
                                x: 1814,
                                y: 967 - 198 - 15,
                            },
                        );
                    }
                    (true, true, true) => {
                        self.bitmaps.qr_code_kb().draw(buffer);
                        // To show both QR codes at the same time we need to move one up a bit
                        // By the height (198) and some extra padding
                        self.bitmaps.qr_code_side_pd().draw_pos(
                            buffer,
                            Point {
                                x: 1814,
                                y: 967 - 198 - 15,
                            },
                        );
                    }
                };
            }
            _ => {
                self.bitmaps.qr_code_3().draw(buffer);
            }
        }

        // Draw input module positions over laptop
        if matches!(
            self.frame,
            StateFrame::TouchpadButNoInputmodules
                | StateFrame::InputModulesMissing
                | StateFrame::NoTouchpad
                | StateFrame::OutdatedFirmware(_)
                | StateFrame::CheckingDiagnostics
                | StateFrame::Finished
        ) {
            self.draw_inputmodules(buffer, input_deck)
        }

        // Help text on the bottom
        match &self.frame {
            StateFrame::NoExpansionBay(BayIssue::BadConnection) => {
                self.bitmaps.bay_bad_connection_text().draw(buffer);
            }
            StateFrame::NoExpansionBay(BayIssue::BothFans) => {
                self.bitmaps.fans_missing_text().draw(buffer);
            }
            StateFrame::NoExpansionBay(BayIssue::InterposerOpen) => {
                self.bitmaps.interposer_door_open_text().draw(buffer);
            }
            StateFrame::NoExpansionBay(BayIssue::LeftFan) => {
                self.bitmaps.left_fan_not_detected_text().draw(buffer);
            }
            StateFrame::NoExpansionBay(BayIssue::ModuleMissing) => {
                self.bitmaps.module_missing_text().draw(buffer);
            }
            StateFrame::NoExpansionBay(BayIssue::RightFan) => {
                self.bitmaps.right_fan_not_detected_text().draw(buffer);
            }
            // StateFrame::NoExpansionBay(issue) => {
            //     draw_textbox(
            //         buffer,
            //         Point::new(WARN_X, WARN_Y),
            //         TextColor::Warn,
            //         &issue.to_string(),
            //     );
            // }
            StateFrame::InterposerClosed => {
                self.bitmaps.interposer_closed_text().draw(buffer);
                // draw_textbox(
                //     buffer,
                //     Point::new(WARN_X, WARN_Y),
                //     TextColor::Warn,
                //     "Expansion Bay Interposer Door Closed - Reboot Required",
                // );
            }
            StateFrame::NoHubboard => {
                self.bitmaps.hubboard_missing_text().draw(buffer);
                // draw_textbox(
                //     buffer,
                //     Point::new(WARN_X, WARN_Y),
                //     TextColor::Warn,
                //     "Input Module Connector Board not detected",
                // );
            }
            StateFrame::TouchpadButNoInputmodules => {
                self.bitmaps.inputmodules_not_detected_text().draw(buffer);
                // draw_textbox(
                //     buffer,
                //     Point::new(WARN_X, WARN_Y),
                //     TextColor::Warn,
                //     "Input Module not detected",
                // );
            }
            StateFrame::InputModulesMissing => {
                self.bitmaps.inputmodules_not_detected_text().draw(buffer);
                // draw_textbox(
                //     buffer,
                //     Point::new(WARN_X, WARN_Y),
                //     TextColor::Warn,
                //     "Input Module not detected",
                // );
            }
            StateFrame::NoTouchpad => {
                self.bitmaps.touchpad_not_detected_text().draw(buffer);
                // draw_textbox(
                //     buffer,
                //     Point::new(WARN_X, WARN_Y),
                //     TextColor::Warn,
                //     "Touchpad not detected",
                // );
            }
            StateFrame::OutdatedFirmware(msgs) => {
                let mut all_msgs = msgs.keyboard.clone();
                if let Some(msg) = &msgs.pd1_controller {
                    all_msgs.push(msg.to_string());
                }
                if let Some(msg) = &msgs.pd2_controller {
                    all_msgs.push(msg.to_string());
                }
                if let Some(msg) = &msgs.pd3_controller {
                    all_msgs.push(msg.to_string());
                }

                let msg = all_msgs.join("\n");
                draw_textbox(buffer, Point::new(WARN_X, WARN_Y), TextColor::Warn, &msg);
            }
            StateFrame::CheckingDiagnostics => {
                draw_textbox(
                    buffer,
                    Point::new(WARN_X, WARN_Y),
                    TextColor::Warn,
                    "Checking fan status...",
                );
            }
            StateFrame::Finished => {
                self.bitmaps.inputmodules_touchpad_ok_text().draw(buffer);
                // draw_textbox(
                //     buffer,
                //     Point::new(WARN_X, WARN_Y),
                //     TextColor::Ok,
                //     "Input Modules and Touchpad detected",
                // );
            }
        }
    }

    fn draw_inputmodules(&mut self, buffer: &mut Buffer, input_deck: &InputDeckStatus) {
        // Full width location can have pogo connection on any location. For other modules we
        // assume it's connected at the very left of the module. But because full-width covers the
        // whole area, it has flexibility of connector position.
        if [
            input_deck.top_row.pos0,
            input_deck.top_row.pos1,
            input_deck.top_row.pos2,
            input_deck.top_row.pos3,
            input_deck.top_row.pos4,
        ]
        .contains(&InputModuleType::FullWidth)
        {
            // C1 spacer in left-most location
            self.bitmaps.c1_module().draw(buffer);
            // Keyboard in the middle
            self.bitmaps.keyboard().draw(buffer);
            // C1 spacer in right-most location
            self.bitmaps.c1_module().draw_pos(
                buffer,
                Point {
                    x: 829 + 84 + 736,
                    y: 583,
                },
            );
        }
        if input_deck.top_row.pos0 == InputModuleType::GenericC {
            self.bitmaps.c1_module().draw(buffer);
        }
        if input_deck.top_row.pos1 == InputModuleType::GenericC {
            self.bitmaps.c1_module().draw_pos(
                buffer,
                Point {
                    x: 829 + 84,
                    y: 583,
                },
            );
        }
        if input_deck.top_row.pos3 == InputModuleType::GenericC {
            self.bitmaps.c1_module().draw_pos(
                buffer,
                Point {
                    x: 829 + 736,
                    y: 583,
                },
            );
        }
        if input_deck.top_row.pos4 == InputModuleType::GenericC {
            self.bitmaps.c1_module().draw_pos(
                buffer,
                Point {
                    x: 829 + 84 + 736,
                    y: 583,
                },
            );
        }

        if input_deck.top_row.pos0 == InputModuleType::GenericA
            || input_deck.top_row.pos0 == InputModuleType::KeyboardA
        {
            self.bitmaps
                .keyboard()
                .draw_pos(buffer, Point { x: 829, y: 583 });
        }
        if input_deck.top_row.pos1 == InputModuleType::GenericA
            || input_deck.top_row.pos1 == InputModuleType::KeyboardA
        {
            self.bitmaps.keyboard().draw(buffer);
        }
        if input_deck.top_row.pos2 == InputModuleType::GenericA
            || input_deck.top_row.pos2 == InputModuleType::KeyboardA
        {
            self.bitmaps.keyboard().draw_pos(
                buffer,
                Point {
                    x: 829 + 84 * 2,
                    y: 583,
                },
            );
        }

        if input_deck.top_row.pos0 == InputModuleType::GenericB {
            self.bitmaps.b1_module().draw(buffer);
        }
        if input_deck.top_row.pos0 == InputModuleType::KeyboardB {
            self.bitmaps.numpad().draw(buffer);
        }
        if input_deck.top_row.pos3 == InputModuleType::GenericB {
            self.bitmaps.b1_module().draw_pos(
                buffer,
                Point {
                    x: 829 + 736,
                    y: 583,
                },
            );
        }
        if input_deck.top_row.pos3 == InputModuleType::KeyboardB {
            self.bitmaps.numpad().draw_pos(
                buffer,
                Point {
                    x: 829 + 736,
                    y: 583,
                },
            );
        }

        if input_deck.touchpad_present {
            let left_kb = matches!(
                input_deck.top_row.pos0,
                InputModuleType::GenericA | InputModuleType::KeyboardA
            );
            let right_kb = matches!(
                input_deck.top_row.pos2,
                InputModuleType::GenericA | InputModuleType::KeyboardA
            );
            if left_kb {
                self.bitmaps.touchpad().draw_pos(
                    buffer,
                    Point {
                        x: 912 - 83,
                        y: 874,
                    },
                );
                self.bitmaps.c2_module().draw_pos(
                    buffer,
                    Point {
                        x: 829 + 735,
                        y: 874,
                    },
                );
                self.bitmaps.c2_module().draw_pos(
                    buffer,
                    Point {
                        x: 829 + 83 + 735,
                        y: 874,
                    },
                );
            } else if right_kb {
                self.bitmaps.c2_module().draw(buffer);
                self.bitmaps.c2_module().draw_pos(
                    buffer,
                    Point {
                        x: 829 + 83,
                        y: 874,
                    },
                );
                self.bitmaps.touchpad().draw_pos(
                    buffer,
                    Point {
                        x: 912 + 83,
                        y: 874,
                    },
                );
            } else {
                // Middle is the default case
                self.bitmaps.c2_module().draw(buffer);
                self.bitmaps.touchpad().draw(buffer);
                self.bitmaps.c2_module().draw_pos(
                    buffer,
                    Point {
                        x: 829 + 83 + 736 - 1,
                        y: 874,
                    },
                );
            }
        }
    }
}

fn has_bay_issue(
    expansion_bay: &EcResponseExpansionBayStatus,
    hw_diag: &EcResponseGetHwDiag,
) -> Option<BayIssue> {
    // Latch is the most important one, since the user can change it live
    // Once they close it, we can show status of the other expansion bay issues
    if !expansion_bay.hatch_switch_closed() {
        return Some(BayIssue::InterposerOpen);
    }

    // Module presence is second most important, since without a module there won't be any fans
    // The DVT1 the UMA fan module does not have board ID resistors, so we can't detect it.
    // We just have to rely on the fans.
    // Since DVT2 UMA module we can check it
    match expansion_bay.expansion_bay_board() {
        Err(ExpansionBayIssue::NoModule) => return Some(BayIssue::ModuleMissing),
        Err(ExpansionBayIssue::BadConnection(_, _)) => return Some(BayIssue::BadConnection),
        Ok(_) => {}
    }

    // Only report fan faults after EC has completed device diagnostics.
    // The EC starts with fault bits set and only clears them after confirming
    // fans are spinning (takes ~2-5s from chipset resume).
    if hw_diag.device_complete != 0 {
        match hw_diag.fan_fault() {
            (true, true) => return Some(BayIssue::BothFans),
            (false, true) => return Some(BayIssue::RightFan),
            (true, false) => return Some(BayIssue::LeftFan),
            (false, false) => {}
        }
    }

    None
}

impl Platform for State {
    type Snapshot = Snapshot;

    fn show_ui() -> bool {
        // Show the UI if we can't tell the BIOS settings or they indicate we need to show the UI
        status::show_ui() != Some(false)
    }

    fn print_status() {
        status::print_module_status();
    }

    fn new(ec: &Option<CrosEc>) -> Self {
        State::new(ec)
    }

    fn read(&self, ec: &Option<CrosEc>) -> Snapshot {
        Snapshot {
            input_deck: ec::get_input_deck(ec),
            expansion_bay: ec::get_expansion_bay(ec),
            hw_diag: ec_comm::get_hw_diag(ec),
            fan_speed: ec::get_fan_speed(ec),
            power_info: ec
                .as_ref()
                .and_then(power::power_info)
                .map(Into::<ReducedPowerInfo>::into),
            usb_devices: uefi_usb::get_usb_devices_noerr(),
        }
    }

    fn mock(&mut self, keypress: Option<Key>, s: &mut Snapshot) {
        ec::keypress_mock_data(
            keypress,
            &mut s.input_deck,
            &mut s.expansion_bay,
            &mut s.hw_diag,
            &mut s.power_info,
        );
        s.usb_devices = uefi_usb::get_usb_devices_noerr();
    }

    fn boot_immediately(&self, snapshot: &Snapshot) -> bool {
        self.next_state(None, snapshot) == StateFrame::Finished
    }

    /// If the any firmware (keyboard or PD) is outdated, start a countdown timer and continue boot
    /// Outdated keyboard or PD firmware does not impact boot functionality, so we can safely continue
    fn tick(&mut self) -> Tick {
        let mut force_redraw = false;
        if let StateFrame::OutdatedFirmware(_) = &self.frame {
            if let Ok(now) = uefi::runtime::get_time() {
                let current_seconds = now.second() as u32 + (now.minute() as u32 * 60);

                if now.second() as u32 != self.last_second {
                    self.last_second = now.second() as u32;
                    force_redraw = true;
                }

                let start_sec = *self.warning_start_time.get_or_insert(current_seconds);
                let elapsed = if current_seconds >= start_sec {
                    current_seconds - start_sec
                } else {
                    // Because we're using the clock time not a timestamp, if the timer was started
                    // during the previous hour, the delta is negative.
                    // So push the elapsed time into same hour as start time
                    // TODO: Upgrade uefi-rs and use timestamp instead of clock time
                    (current_seconds + 3600) - start_sec
                };

                let remaining = BOOT_TIME_SECONDS.saturating_sub(elapsed);
                self.countdown_text = format!("\nContinuing boot in {}s...", remaining);

                if elapsed >= BOOT_TIME_SECONDS {
                    info!("Timeout reached for firmware warning. Continuing boot...");
                    return Tick::Boot;
                }
            }
        } else {
            // Reset the timer in case we exit the OutdatedFirmware state and enter again later
            self.warning_start_time = None;
            self.countdown_text.clear();
        }
        Tick::Continue { force_redraw }
    }

    fn handle_key(&mut self, key: Key) -> bool {
        let p_key = Char16::try_from('p').unwrap();
        match key {
            Key::Printable(key) if key == p_key => {
                self.mock_outdated_fw = !self.mock_outdated_fw;
                true
            }
            _ => false,
        }
    }

    fn on_short_press(&mut self) {
        if self.frame == StateFrame::InterposerClosed {
            // EfiResetCold does a warm reset on AMD systems,
            // so we need to manually force a cold reset.
            // Use legacy CF9 register - AMD doc says it keeps CPU in S5 for 3 seconds.
            Pio::<u8>::new(0x0CF9).write(0x0E);
            // If it reaches here, it will panic, show an error message and shut off
            // It's safe and customers can report this issue, if it occurs
            unreachable!();
        }
    }

    fn need_redraw(&self, prev: Option<&Snapshot>, cur: &Snapshot) -> bool {
        let new_deck = Some(&cur.input_deck) != prev.map(|p| &p.input_deck);
        let new_bay = Some(&cur.expansion_bay) != prev.map(|p| &p.expansion_bay);
        let new_diag = Some(&cur.hw_diag) != prev.map(|p| &p.hw_diag);
        let new_power = Some(&cur.power_info) != prev.map(|p| &p.power_info);
        let new_usb_devices = Some(&cur.usb_devices) != prev.map(|p| &p.usb_devices);

        if new_deck {
            debug!("Deck Changed");
            debug!("Old: {:?}", prev.map(|p| &p.input_deck));
            debug!("New: {:?}", cur.input_deck);
        }
        if new_bay {
            debug!("Bay Changed");
            debug!("Old: {:?}", prev.map(|p| &p.expansion_bay));
            debug!("New: {:?}", cur.expansion_bay);
        }
        if new_diag {
            debug!("Diag Changed");
            debug!("Old: {:?}", prev.map(|p| &p.hw_diag));
            debug!("New: {:?}", cur.hw_diag);
        }
        if new_power {
            debug!("Power Changed");
            debug!("Old: {:?}", prev.map(|p| &p.power_info));
            debug!("New: {:?}", cur.power_info);
        }
        if new_usb_devices {
            debug!("USB Devices Changed");
            debug!("Old: {:?}", prev.map(|p| &p.usb_devices));
            debug!("New: {:?}", cur.usb_devices);
        }

        new_deck || new_bay || new_diag || new_power || new_usb_devices
    }
    fn draw(
        &mut self,
        buffer: &mut Buffer,
        prev: Option<&Snapshot>,
        cur: &Snapshot,
        info: &LoopInfo,
        verbose: bool,
    ) {
        self.frame = self.next_state(prev, cur);
        self.draw_state(buffer, &cur.input_deck);

        if let StateFrame::OutdatedFirmware(msgs) = &self.frame {
            let mut all_msgs = msgs.keyboard.clone();
            if let Some(msg) = &msgs.pd1_controller {
                all_msgs.push(msg.to_string());
            }
            if let Some(msg) = &msgs.pd2_controller {
                all_msgs.push(msg.to_string());
            }
            if let Some(msg) = &msgs.pd3_controller {
                all_msgs.push(msg.to_string());
            }

            let mut msg = all_msgs.join("\n");
            msg.push_str(&self.countdown_text);

            draw_textbox(buffer, Point::new(WARN_X, WARN_Y), TextColor::Warn, &msg);
        }

        let expansion_bay = &cur.expansion_bay;
        draw_battery_info(buffer, Point::new(WIDTH_I32 / 2, 0), &cur.power_info);
        draw_version(
            buffer,
            Some(DebugInfo {
                bios_version: self.bios_version.clone(),
                ec_version: self.ec_version.clone(),
                power_button: info.power_button,
                ec_present: info.ec_present,
                long_press: info.long_press,
                bay_module: expansion_bay.expansion_bay_board(),
                gpu0: expansion_bay.board_id_0,
                gpu1: expansion_bay.board_id_1,
                bay_state: expansion_bay.state,
                bay_enabled: expansion_bay.module_enabled(),
                bay_fault: expansion_bay.module_fault(),
                hatch_closed: expansion_bay.hatch_switch_closed(),
                hw_diag: cur.hw_diag,
                fan_speed: cur.fan_speed,
                deck: cur.input_deck.clone(),
                power_info: cur.power_info.clone(),
                pd1_ver: self.pd1_ver,
                pd2_ver: self.pd2_ver,
                pd3_ver: self.pd3_ver,
                usb_devices: cur.usb_devices.clone(),
            }),
            verbose,
        );
    }
}
