// TODO: For when I ported the font
// use alloc::string::ToString;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;

use embedded_graphics::geometry::Size;
use embedded_graphics::prelude::Point;

use framework_lib::ccgx::{AppVersion, Application, MainPdVersions};
use framework_lib::chromium_ec::commands::{
    EcResponseExpansionBayStatus, EcResponseGetHwDiag, ExpansionBayBoard, ExpansionBayIssue,
};
use framework_lib::chromium_ec::input_deck::InputDeckStatus;
use framework_lib::chromium_ec::input_deck::{InputDeckState, InputModuleType};
use framework_lib::chromium_ec::CrosEc;
use framework_lib::power::{self, ReducedPowerInfo};
#[allow(unused_imports)]
use log::{debug, error, info, trace};
use log::{log_enabled, Level};
use uefi::prelude::{BootServices, RuntimeServices};
use uefi::proto::console::gop::GraphicsOutput;
use uefi::proto::console::text::{Input, Key, ScanCode};
use uefi::table::boot::{OpenProtocolAttributes, OpenProtocolParams, ScopedProtocol, SearchType};
use uefi::table::runtime::{VariableAttributes, VariableVendor};
use uefi::table::{Boot, SystemTable};
use uefi::{cstr16, guid, Char16, Identify};
#[allow(unused_imports)]
use uefi_services::{print, println, system_table};

use hwio::{Io, Pio};

use crate::buffer::Buffer;
use crate::built_info;
use crate::drawing::*;
use crate::ec_comm;
use crate::sleep::sleep_or;
use crate::uefi_usb::{self, UsbDevice};

const FAILED_INPUT: &str = "Failed to get keypress";

const EFI_OS_INDICATIONS_BOOT_TO_FW_UI: u64 = 0x0000000000000001;
const EFI_GLOBAL_VARIABLE_GUID: uefi::Guid = guid!("8be4df61-93ca-11d2-aa0d-00e098032b8c");

pub const PD3_VER: AppVersion = AppVersion {
    application: Application::Notebook,
    major: 0,
    minor: 0,
    circuit: 0x21,
};

#[allow(dead_code)]
pub struct DebugInfo {
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
    pub deck: InputDeckStatus,
    pub power_info: Option<ReducedPowerInfo>,
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
    pd3_ver: Option<AppVersion>,
    fw_outdated_msg: Option<String>,
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
    OutdatedFirmware(Vec<String>),
    Finished,
}

impl State {
    pub fn new(ec: &Option<CrosEc>) -> Self {
        let mut fw_outdated_msg = None;
        let mut pd3_ver = None;
        let versions = ec.as_ref().and_then(|x| power::read_pd_version(x).ok());
        if let Some(MainPdVersions::Many(versions)) = versions {
            if let Some(ver) = versions.get(2) {
                pd3_ver = Some(ver.app);
                if ver.app != PD3_VER {
                    fw_outdated_msg = Some(format!(
                        "Please update the dGPU PD firmware. Is {}, expected: {}.",
                        ver.app, PD3_VER
                    ));
                }
            }
        }
        #[cfg(feature = "qemu")]
        {
            fw_outdated_msg = Some(format!(
                "Please update the dGPU PD firmware. Is: ??.??.??, expected: {}.",
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
            pd3_ver,
            fw_outdated_msg,
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
                let mut outdated_fw_msgs = vec![];

                // In QEMU force an error message
                #[cfg(feature = "qemu")]
                outdated_fw_msgs.push(format!(
                    "Firmware of {} is outdated. Please update to v0.3.0 or later.",
                    "Framework 16 - ANSI Keyboard"
                ));

                for dev in usb_devices {
                    // Show a warning of older than 0.3.0
                    if dev.is_framework_keyboard() && dev.bcd_device < 0x0030 {
                        let name = dev.get_name().unwrap_or("???").trim();
                        outdated_fw_msgs.push(format!(
                            "Firmware of {} is outdated. Please update to v0.3.0 or later.",
                            name
                        ));
                    }
                }

                if let Some(msg) = &self.fw_outdated_msg {
                    outdated_fw_msgs.push(msg.clone());
                }

                if !outdated_fw_msgs.is_empty() {
                    StateFrame::OutdatedFirmware(outdated_fw_msgs)
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
            StateFrame::Finished | StateFrame::OutdatedFirmware(_) => {
                // Boot Button indicator right
                self.bitmaps.button_boot().draw(buffer);

                // Overlay latches closed
                self.bitmaps.latches_closed_left().draw(buffer);
                self.bitmaps.latches_closed_right().draw(buffer);
            }
            StateFrame::InterposerClosed => {
                self.bitmaps.button_short_reboot().draw(buffer);
                self.bitmaps.button_long_off().draw(buffer);
            }
            _ => {
                // Button indicators right
                self.bitmaps.button_short_skip().draw(buffer);
                self.bitmaps.button_long_off().draw(buffer);
            }
        }

        // QR Codes
        match self.frame {
            StateFrame::NoExpansionBay(_) | StateFrame::InterposerClosed => {
                self.bitmaps.qr_code_1().draw(buffer);
            }
            StateFrame::NoHubboard => {
                self.bitmaps.qr_code_2().draw(buffer);
            }
            StateFrame::Finished | StateFrame::OutdatedFirmware(_) => {}
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
            StateFrame::OutdatedFirmware(msg) => {
                let msg = msg.join("\n");
                draw_textbox(buffer, Point::new(WARN_X, WARN_Y), TextColor::Warn, &msg);
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
        if input_deck.top_row.pos0 == InputModuleType::FullWidth {
            self.bitmaps.full_width().draw(buffer);
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

fn get_gop_buffer(bs: &BootServices) -> Option<(ScopedProtocol<GraphicsOutput>, Buffer)> {
    let gop_handles = if let Ok(gop_handles) =
        bs.locate_handle_buffer(SearchType::ByProtocol(&GraphicsOutput::GUID))
    {
        gop_handles
    } else {
        error!("GraphicsOutput protocol is missing");
        return None;
    };

    let gop = if let Some(handle) = gop_handles.iter().next() {
        unsafe {
            bs.open_protocol::<GraphicsOutput>(
                OpenProtocolParams {
                    handle: *handle,
                    agent: bs.image_handle(),
                    controller: None,
                },
                OpenProtocolAttributes::GetProtocol,
            )
            .expect("Failed to open GraphicsOutput handle")
        }
    } else {
        error!("No GOP handles found");
        return None;
    };

    let mode = gop.current_mode_info();
    let (width, height) = mode.resolution();
    let _size = Point::new(width as i32, height as i32);
    info!("Current Resolution {}x{}", width, height);

    // Create a buffer to draw into.
    let (width, height) = gop.current_mode_info().resolution();
    Some((gop, Buffer::new(width, height)))
}

fn get_input_protocol(bs: &BootServices) -> Option<ScopedProtocol<Input>> {
    let input_handles =
        if let Ok(input_handles) = bs.locate_handle_buffer(SearchType::ByProtocol(&Input::GUID)) {
            input_handles
        } else {
            error!("Input protocol is missing");
            return None;
        };

    if let Some(handle) = input_handles.iter().next() {
        unsafe {
            bs.open_protocol::<Input>(
                OpenProtocolParams {
                    handle: *handle,
                    agent: bs.image_handle(),
                    controller: None,
                },
                OpenProtocolAttributes::GetProtocol,
            )
            .ok()
        }
    } else {
        info!("No Input handles found");
        None
    }
}

// Avoid using because it might cause trouble with later boot process
fn _get_exclusive_input_protocol(bs: &BootServices) -> Option<ScopedProtocol<Input>> {
    let input_handle = bs.get_handle_for_protocol::<Input>().ok()?;
    bs.open_protocol_exclusive::<Input>(input_handle).ok()
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

    match hw_diag.fan_fault() {
        (true, true) => return Some(BayIssue::BothFans),
        (false, true) => return Some(BayIssue::RightFan),
        (true, false) => return Some(BayIssue::LeftFan),
        (false, false) => {}
    }

    None
}

/// Set OS indications so that a reboot will boot into setup menu
fn set_os_indications_setupmenu(rt: &RuntimeServices) {
    let vendor = VariableVendor(EFI_GLOBAL_VARIABLE_GUID);
    let mut buf = [0; 8];
    rt.get_variable(cstr16!("OsIndicationsSupported"), &vendor, &mut buf)
        .expect("failed to get variable");
    let os_indications: u64 = u64::from_ne_bytes(buf);
    debug!("OS Indications: {:?}", os_indications);

    let os_indications = if rt
        .get_variable(cstr16!("OsIndications"), &vendor, &mut buf)
        .is_ok()
    {
        u64::from_ne_bytes(buf) | EFI_OS_INDICATIONS_BOOT_TO_FW_UI
    } else {
        EFI_OS_INDICATIONS_BOOT_TO_FW_UI
    };
    debug!("OS Indications: {:?}", os_indications);

    let attrs = VariableAttributes::NON_VOLATILE
        | VariableAttributes::BOOTSERVICE_ACCESS
        | VariableAttributes::RUNTIME_ACCESS;
    rt.set_variable(
        cstr16!("OsIndications"),
        &vendor,
        attrs,
        &u64::to_ne_bytes(os_indications),
    )
    .expect("failed to set variable");
}

pub fn start(
    bs: &BootServices,
    st: &SystemTable<Boot>,
    ec: &Option<CrosEc>,
    force_ui: bool,
) -> uefi::Result<Option<uefi::table::runtime::ResetType>> {
    let mut input_deck = ec_comm::get_input_deck(ec);
    let mut expansion_bay = ec_comm::get_expansion_bay(ec);
    let mut hw_diag = ec_comm::get_hw_diag(ec);
    let mut power_info: Option<ReducedPowerInfo> = ec
        .as_ref()
        .and_then(power::power_info)
        .map(Into::<ReducedPowerInfo>::into);
    let mut usb_devices = uefi_usb::get_usb_devices_noerr(bs);

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

    if !matches!(
        input_deck.state,
        InputDeckState::Disconnected | InputDeckState::TurningOn | InputDeckState::On
    ) {
        println!("Input Deck State Overridden: {:?}. No detection of modules possible - continuing to boot ", input_deck.state);
        bs.stall(1_000_000);
        return Ok(None);
    }

    // Get GOP protocol to draw on the screen
    let (mut gop, mut buffer) = if let Some((gop, buf)) = get_gop_buffer(bs) {
        (gop, buf)
    } else {
        error!("inputmodules.efi - Couldn't draw to display");
        // Something went wrong with GOP initialization, keep booting
        // TODO: Maybe in DVT2 we should still assert here?
        return Ok(None);
    };

    // Get Input protocol to detect keypresses
    let mut input = get_input_protocol(bs).expect("Should always have input protocol");

    let mut state = State::new(ec);
    let mut long_press = 0;
    loop {
        #[cfg(feature = "azalea")]
        let azalea = true;
        #[cfg(not(feature = "azalea"))]
        let azalea = false;
        if !azalea && ec.is_some() {
            input_deck = ec_comm::get_input_deck(ec);
            expansion_bay = ec_comm::get_expansion_bay(ec);
            hw_diag = ec_comm::get_hw_diag(ec);
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
        usb_devices = uefi_usb::get_usb_devices_noerr(bs);
        // TODO: Maybe mock power button press?
        let button_pressed = ec_comm::button_pressed(ec);

        // Detect short press of power button
        // Short press (or ESC) will exit and boot (skip all further checks)
        let short_press = (!button_pressed && state.prev_pressed && long_press == 1)
            || state.keypress == Some(Key::Special(ScanCode::ESCAPE));
        match (short_press, state.frame.clone()) {
            // Reboot
            (true, StateFrame::InterposerClosed) => {
                // EfiResetCold does a warm reset on AMD systems,
                // so we need to manually force a cold reset.
                // Use legacy CF9 register - AMD doc says it keeps CPU in S5 for 3 seconds.
                Pio::<u8>::new(0x0CF9).write(0x0E);
                // If it reaches here, it will panic, show an error message and shut off
                // It's safe and customers can report this issue, if it occurs
                unreachable!();
            }
            // Just exit and continue
            (true, _) => break,
            _ => {}
        }

        if state.keypress == Some(Key::Special(ScanCode::FUNCTION_12)) {
            let rt = st.runtime_services();
            set_os_indications_setupmenu(rt);
            info!("Rebooting into BIOS Setup Menu");
            return Ok(Some(uefi::table::runtime::ResetType::Cold));
        }

        let need_redraw = state.need_redraw(
            &input_deck,
            &expansion_bay,
            &hw_diag,
            &power_info,
            &usb_devices,
        );
        if need_redraw && log_enabled!(Level::Debug) {
            // Sleep after state.need_redraw prints debug information
            bs.stall(1_000_000);
        }

        let q_key = Char16::try_from('?').unwrap();
        let need_redraw = match state.keypress {
            Some(Key::Printable(key)) if key == q_key => {
                state.verbose = !state.verbose;
                true
            }
            _ => need_redraw,
        };

        // Detect long press of power button
        let need_redraw = if button_pressed && state.prev_pressed && !need_redraw {
            if long_press > 4 {
                // Exit and signal that we need to poweroff
                info!("Powering off");
                return Ok(Some(uefi::table::runtime::ResetType::Shutdown));
            }
            // Wait a second
            sleep_or(bs, 1_000, unsafe {
                Some(input.wait_for_key_event().unsafe_clone())
            });
            long_press += 1;
            true
        } else {
            long_press = 0;
            need_redraw
        };

        if need_redraw {
            state.frame = state.next_state(&input_deck, &expansion_bay, &hw_diag, &usb_devices);
            state.draw_state(&mut buffer, &input_deck);

            draw_battery_info(&mut buffer, &power_info);
            draw_version(
                &mut buffer,
                Some(DebugInfo {
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
                    deck: input_deck.clone(),
                    power_info: power_info.clone(),
                    pd3_ver: state.pd3_ver,
                    usb_devices: usb_devices.clone(),
                }),
                state.verbose,
            );
            buffer.blit(&mut gop)?;
        }

        // TODO: It needs to be very low to detect short power button presses,
        // but it doesn't have to be that low for the input modules. Maybe we can find a compromise
        // Put CPU to sleep until key press or 10ms timeout
        sleep_or(bs, 10, unsafe {
            Some(input.wait_for_key_event().unsafe_clone())
        });
        state.keypress = input.read_key().expect(FAILED_INPUT);

        // Save current values to access and compare them in next iteration
        state.prev_input_deck = Some(input_deck.clone());
        state.prev_expansion_bay = Some(expansion_bay);
        state.prev_hw_diag = Some(hw_diag);
        state.prev_power_info = Some(power_info.clone());
        state.prev_usb_devices = Some(usb_devices.clone());
        state.prev_pressed = button_pressed;
    }
    let _ = clear_screen(&mut buffer, &mut gop);
    info!("Exited loop, continuing to boot");
    Ok(None)
}
