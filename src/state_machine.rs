// TODO: For when I ported the font
// use alloc::string::ToString;
use crate::alloc::string::ToString;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use embedded_graphics::draw_target::{DrawTarget, DrawTargetExt};
use embedded_graphics::geometry::Size;
use embedded_graphics::pixelcolor::Rgb888;
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
use framework_lib::smbios::{get_platform, Platform};
#[allow(unused_imports)]
use log::{debug, error, info, trace};
use log::{log_enabled, Level};
use uefi::proto::console::text::{Key, ScanCode};
use uefi::Char16;

use crate::buffer::Buffer;
use crate::built_info;
use crate::drawing::*;
use crate::ec_comm;
use crate::platform::{self, Screen};
use crate::usb::{self, UsbDevice};

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

pub struct State {
    // Cache bitmaps for drawing
    bitmaps: StateBitmaps,
    screen_size: Size,
    // Currently displayed frame
    frame: StateFrame,
    prev_input_deck: Option<InputDeckStatus>,
    prev_expansion_bay: Option<EcResponseExpansionBayStatus>,
    prev_hw_diag: Option<EcResponseGetHwDiag>,
    prev_power_info: Option<Option<ReducedPowerInfo>>,
    prev_usb_devices: Option<Vec<UsbDevice>>,
    prev_pressed: bool,
    verbose: bool,
    keypress: Option<Key>,
    pd1_ver: Option<AppVersion>,
    pd2_ver: Option<AppVersion>,
    pd3_ver: Option<AppVersion>,
    outdated_firmware: OutdatedFirmware,
    mock_outdated_fw: bool,
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
    pub fn new(ec: &Option<CrosEc>) -> Self {
        let mut outdated_firmware = OutdatedFirmware::default();
        let mut pd1_ver = None;
        let mut pd2_ver = None;
        let mut pd3_ver = None;
        let versions = ec.as_ref().and_then(|x| power::read_pd_version(x).ok());
        if let Some(MainPdVersions::Many(versions)) = versions {
            if let Some(Platform::Framework16Amd7080) = get_platform() {
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
            prev_input_deck: None,
            prev_expansion_bay: None,
            prev_hw_diag: None,
            prev_power_info: None,
            prev_usb_devices: None,
            prev_pressed: false,
            verbose: built_info::DEBUG,
            keypress: None,
            pd1_ver,
            pd2_ver,
            pd3_ver,
            outdated_firmware,
            mock_outdated_fw: false,
        }
    }

    // Very simple state machine
    // The next state does not depend on the current state
    fn next_state(
        &self,
        input_deck: &InputDeckStatus,
        expansion_bay: &EcResponseExpansionBayStatus,
        hw_diag: &EcResponseGetHwDiag,
        usb_devices: &Vec<UsbDevice>,
    ) -> StateFrame {
        // Can't get out of this, need to reboot
        if self.frame == StateFrame::InterposerClosed {
            return self.frame.clone();
        }

        let prev_bay_issue = if let (Some(ref prev_expansion_bay), Some(ref pre_hw_diag)) =
            (self.prev_expansion_bay, self.prev_hw_diag)
        {
            has_bay_issue(prev_expansion_bay, pre_hw_diag)
        } else {
            None
        };
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

        // Move the laptop over to the left when showing the debug panel on the right
        let mut scene = buffer.translated(scene_offset(self.verbose));

        // Draw laptop with latches open
        self.bitmaps.laptop_latches_open().draw(&mut scene);

        // Fix up the laptop
        match self.frame {
            StateFrame::NoExpansionBay(BayIssue::InterposerOpen) | StateFrame::InterposerClosed => {
                self.bitmaps.latch_warning().draw(&mut scene)
            }
            StateFrame::NoExpansionBay(_) => self.bitmaps.expansion_bay_missing().draw(&mut scene),
            StateFrame::NoHubboard => self.bitmaps.hubboard_missing().draw(&mut scene),
            _ => {}
        }

        // Overlay latches closed
        if matches!(
            self.frame,
            StateFrame::Finished | StateFrame::OutdatedFirmware(_)
        ) {
            self.bitmaps.latches_closed_left().draw(&mut scene);
            self.bitmaps.latches_closed_right().draw(&mut scene);
        }

        // QR Codes
        match &self.frame {
            StateFrame::NoExpansionBay(_) | StateFrame::InterposerClosed => {
                self.bitmaps.qr_code_1().draw(&mut scene);
            }
            StateFrame::NoHubboard => {
                self.bitmaps.qr_code_2().draw(&mut scene);
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
                        self.bitmaps.qr_code_pd().draw(&mut scene);
                    }
                    (false, true, false) => {
                        self.bitmaps.qr_code_side_pd().draw(&mut scene);
                    }
                    // If dGPU and side PD both are outdated, show only the side PD QR code for two reasons.
                    // 1. They both redirect to the same KB article - no need to duplicate.
                    // 2. Side PD not in sync with BIOS version means there was a bug during the BIOS update.
                    // We use the side PD QR code to track who experienced the updater bug, so prioritize that QR over the other one
                    (false, true, true) => {
                        self.bitmaps.qr_code_side_pd().draw(&mut scene);
                    }
                    (true, false, false) => {
                        self.bitmaps.qr_code_kb().draw(&mut scene);
                    }
                    (true, false, true) => {
                        self.bitmaps.qr_code_kb().draw(&mut scene);
                        // To show both QR codes at the same time we need to move one up a bit
                        // By the height (198) and some extra padding
                        self.bitmaps.qr_code_pd().draw_pos(
                            &mut scene,
                            Point {
                                x: 1814,
                                y: 967 - 198 - 15,
                            },
                        );
                    }
                    (true, true, false) => {
                        self.bitmaps.qr_code_kb().draw(&mut scene);
                        // To show both QR codes at the same time we need to move one up a bit
                        // By the height (198) and some extra padding
                        self.bitmaps.qr_code_side_pd().draw_pos(
                            &mut scene,
                            Point {
                                x: 1814,
                                y: 967 - 198 - 15,
                            },
                        );
                    }
                    (true, true, true) => {
                        self.bitmaps.qr_code_kb().draw(&mut scene);
                        // To show both QR codes at the same time we need to move one up a bit
                        // By the height (198) and some extra padding
                        self.bitmaps.qr_code_side_pd().draw_pos(
                            &mut scene,
                            Point {
                                x: 1814,
                                y: 967 - 198 - 15,
                            },
                        );
                    }
                };
            }
            _ => {
                self.bitmaps.qr_code_3().draw(&mut scene);
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
            self.draw_inputmodules(&mut scene, input_deck)
        }

        // Hints on lower left and right
        match self.frame {
            StateFrame::Finished => {
                // Boot Button indicator right
                self.bitmaps.button_boot().draw(buffer);
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

    fn draw_inputmodules<D>(&mut self, buffer: &mut D, input_deck: &InputDeckStatus)
    where
        D: DrawTarget<Color = Rgb888>,
        D::Error: core::fmt::Debug,
    {
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

    pub fn need_redraw(
        &self,
        input_deck: &InputDeckStatus,
        expansion_bay: &EcResponseExpansionBayStatus,
        hw_diag: &EcResponseGetHwDiag,
        power_info: &Option<ReducedPowerInfo>,
        usb_devices: &Vec<UsbDevice>,
    ) -> bool {
        let new_deck = Some(input_deck) != self.prev_input_deck.as_ref();
        let new_bay = Some(expansion_bay) != self.prev_expansion_bay.as_ref();
        let new_diag = Some(hw_diag) != self.prev_hw_diag.as_ref();
        let new_power = Some(power_info) != self.prev_power_info.as_ref();
        let new_usb_devices = Some(usb_devices) != self.prev_usb_devices.as_ref();

        if new_deck {
            debug!("Deck Changed");
            debug!("Old: {:?}", self.prev_input_deck.as_ref());
            debug!("New: {:?}", input_deck);
        }
        if new_bay {
            debug!("Bay Changed");
            debug!("Old: {:?}", self.prev_expansion_bay.as_ref());
            debug!("New: {:?}", expansion_bay);
        }
        if new_diag {
            debug!("Diag Changed");
            debug!("Old: {:?}", self.prev_hw_diag.as_ref());
            debug!("New: {:?}", hw_diag);
        }
        if new_power {
            debug!("Power Changed");
            debug!("Old: {:?}", self.prev_power_info.as_ref());
            debug!("New: {:?}", power_info);
        }
        if new_usb_devices {
            debug!("USB Devices Changed");
            debug!("Old: {:?}", self.prev_usb_devices.as_ref());
            debug!("New: {:?}", usb_devices);
        }

        new_deck || new_bay || new_diag || new_power || new_usb_devices
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

pub fn start(
    ec: &Option<CrosEc>,
    force_ui: bool,
) -> uefi::Result<Option<uefi::runtime::ResetType>> {
    let mut input_deck = ec_comm::get_input_deck(ec);
    let mut expansion_bay = ec_comm::get_expansion_bay(ec);
    let mut hw_diag = ec_comm::get_hw_diag(ec);
    let mut fan_speed = ec_comm::get_fan_speed(ec);
    let mut power_info: Option<ReducedPowerInfo> = ec
        .as_ref()
        .and_then(power::power_info)
        .map(Into::<ReducedPowerInfo>::into);
    let mut usb_devices = usb::get_usb_devices_noerr();

    #[cfg(feature = "qemu")]
    let smbios: Option<SmbiosStore> = None;
    // Without EC (e.g. running natively in mock mode) don't show the host's BIOS version
    #[cfg(not(feature = "qemu"))]
    let smbios = ec.as_ref().and_then(|_| get_smbios());
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

    let state = State::new(ec);
    // If we start out as fully populated we want to boot as fast as possible
    // and skip the GUI
    if !force_ui
        && state.next_state(&input_deck, &expansion_bay, &hw_diag, &usb_devices)
            == StateFrame::Finished
    {
        info!("Started out as fully populated, booting immediately");
        return Ok(None);
    }

    // Get GOP protocol to draw on the screen and Input protocol to detect keypresses
    let mut screen = if let Some(screen) = Screen::open() {
        screen
    } else {
        error!("inputmodules.efi - Couldn't draw to display");
        // Something went wrong with GOP initialization, keep booting
        // TODO: Maybe in DVT2 we should still assert here?
        return Ok(None);
    };
    let (width, height) = screen.size();
    let mut buffer = Buffer::new(width, height);

    let mut state = State::new(ec);
    // Record the time when the warning shows up and the timer starts
    // So that the countdown does not start when the user is still installing inputmodules
    let mut warning_start_time: Option<u32> = None;
    let mut last_second: u32 = 0;
    let mut countdown_text = String::new();
    let mut long_press = 0;
    loop {
        let mut force_tick_redraw = false;
        // If the any firmware (keyboard or PD) is outdated, start a countdown timer and continue boot
        // Outdated keyboard or PD firmware does not impact boot functionality, so we can safely continue
        if let StateFrame::OutdatedFirmware(_) = &state.frame {
            if let Some(current_seconds) = platform::clock_seconds() {
                if current_seconds % 60 != last_second {
                    last_second = current_seconds % 60;
                    force_tick_redraw = true;
                }

                if warning_start_time.is_none() {
                    warning_start_time = Some(current_seconds);
                }

                let start_sec = warning_start_time.unwrap();
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
                countdown_text = format!("\nContinuing boot in {}s...", remaining);

                if elapsed >= BOOT_TIME_SECONDS {
                    info!("Timeout reached for firmware warning. Continuing boot...");
                    break;
                }
            }
        } else {
            // Reset the timer in case we exit the OutdatedFirmware state and enter again later
            warning_start_time = None;
            countdown_text.clear();
        }

        #[cfg(feature = "azalea")]
        let azalea = true;
        #[cfg(not(feature = "azalea"))]
        let azalea = false;
        if !azalea && ec.is_some() {
            input_deck = ec_comm::get_input_deck(ec);
            expansion_bay = ec_comm::get_expansion_bay(ec);
            hw_diag = ec_comm::get_hw_diag(ec);
            fan_speed = ec_comm::get_fan_speed(ec);
            power_info = ec
                .as_ref()
                .and_then(power::power_info)
                .map(Into::<ReducedPowerInfo>::into);
        } else {
            ec_comm::keypress_mock_data(
                state.keypress,
                &mut input_deck,
                &mut expansion_bay,
                &mut hw_diag,
                &mut power_info,
            );
        }
        usb_devices = usb::get_usb_devices_noerr();
        // TODO: Maybe mock power button press?
        let button_pressed = ec_comm::button_pressed(ec);

        // Detect short press of power button
        // Short press (or ESC) will exit and boot (skip all further checks)
        let short_press = (!button_pressed && state.prev_pressed)
            || state.keypress == Some(Key::Special(ScanCode::ESCAPE));
        match (short_press, state.frame.clone()) {
            // Reboot
            (true, StateFrame::InterposerClosed) => {
                // Does not return in UEFI
                platform::force_cold_reset();
                break;
            }
            // Just exit and continue
            (true, _) => break,
            _ => {}
        }

        if state.keypress == Some(Key::Special(ScanCode::FUNCTION_12)) {
            platform::set_os_indications_setupmenu();
            info!("Rebooting into BIOS Setup Menu");
            return Ok(Some(uefi::runtime::ResetType::COLD));
        }

        // If verbose always need to update, to update things like fan info in the debug section
        let need_redraw = force_tick_redraw
            || state.verbose
            || state.need_redraw(
                &input_deck,
                &expansion_bay,
                &hw_diag,
                &power_info,
                &usb_devices,
            );
        if need_redraw && log_enabled!(Level::Debug) {
            // Sleep after state.need_redraw prints debug information
            platform::stall(core::time::Duration::from_micros(1_000_000));
        }

        let q_key = Char16::try_from('?').unwrap();
        let p_key = Char16::try_from('p').unwrap();
        let need_redraw = match state.keypress {
            Some(Key::Printable(key)) if key == q_key => {
                state.verbose = !state.verbose;
                true
            }
            Some(Key::Printable(key)) if key == p_key => {
                state.mock_outdated_fw = !state.mock_outdated_fw;
                true
            }
            _ => need_redraw,
        };

        // Detect long press of power button
        let need_redraw = if button_pressed && state.prev_pressed && !need_redraw {
            if long_press > 4 {
                // Exit and signal that we need to poweroff
                info!("Powering off");
                return Ok(Some(uefi::runtime::ResetType::SHUTDOWN));
            }
            // Wait a second
            screen.wait(1_000);
            long_press += 1;
            true
        } else {
            long_press = 0;
            need_redraw
        };

        if need_redraw {
            state.frame = state.next_state(&input_deck, &expansion_bay, &hw_diag, &usb_devices);
            state.draw_state(&mut buffer, &input_deck);

            if let StateFrame::OutdatedFirmware(msgs) = &state.frame {
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
                msg.push_str(&countdown_text);

                draw_textbox(
                    &mut buffer,
                    Point::new(WARN_X, WARN_Y),
                    TextColor::Warn,
                    &msg,
                );
            }

            draw_battery_info(&mut buffer, &power_info, state.verbose);
            draw_version(
                &mut buffer,
                Some(DebugInfo {
                    bios_version: bios_version.clone(),
                    ec_version: ec_version.clone(),
                    power_button: button_pressed,
                    ec_present: ec.is_some(),
                    long_press,
                    bay_module: expansion_bay.expansion_bay_board(),
                    gpu0: expansion_bay.board_id_0,
                    gpu1: expansion_bay.board_id_1,
                    bay_state: expansion_bay.state,
                    bay_enabled: expansion_bay.module_enabled(),
                    bay_fault: expansion_bay.module_fault(),
                    hatch_closed: expansion_bay.hatch_switch_closed(),
                    hw_diag,
                    fan_speed,
                    deck: input_deck.clone(),
                    power_info: power_info.clone(),
                    pd1_ver: state.pd1_ver,
                    pd2_ver: state.pd2_ver,
                    pd3_ver: state.pd3_ver,
                    usb_devices: usb_devices.clone(),
                }),
                state.verbose,
            );
            screen.blit(&buffer)?;
        }

        // TODO: It needs to be very low to detect short power button presses,
        // but it doesn't have to be that low for the input modules. Maybe we can find a compromise
        // Put CPU to sleep until key press or 10ms timeout
        state.keypress = screen.wait_for_key(10);

        // Save current values to access and compare them in next iteration
        state.prev_input_deck = Some(input_deck.clone());
        state.prev_expansion_bay = Some(expansion_bay);
        state.prev_hw_diag = Some(hw_diag);
        state.prev_power_info = Some(power_info.clone());
        state.prev_usb_devices = Some(usb_devices.clone());
        state.prev_pressed = button_pressed;
    }
    clear_screen(&mut buffer);
    let _ = screen.blit(&buffer);
    info!("Exited loop, continuing to boot");
    Ok(None)
}
