use alloc::string::ToString;
use core::fmt;

use embedded_graphics::geometry::Size;
use embedded_graphics::prelude::Point;

use framework_lib::chromium_ec::commands::{
    EcResponseExpansionBayStatus, EcResponseGetHwDiag, ExpansionBayBoard, ExpansionBayIssue,
};
use framework_lib::chromium_ec::input_deck::InputDeckStatus;
use framework_lib::chromium_ec::input_deck::{InputDeckState, InputModuleType};
use framework_lib::chromium_ec::CrosEc;
use framework_lib::power::{self, PowerInfo};
#[allow(unused_imports)]
use log::{debug, error, info, trace};
use uefi::prelude::BootServices;
use uefi::proto::console::gop::GraphicsOutput;
use uefi::proto::console::text::{Input, Key, ScanCode};
use uefi::table::boot::{OpenProtocolAttributes, OpenProtocolParams, ScopedProtocol, SearchType};
use uefi::table::{Boot, SystemTable};
use uefi::{Char16, Identify};
#[allow(unused_imports)]
use uefi_services::{print, println};

use crate::buffer::Buffer;
use crate::built_info;
use crate::drawing::*;
use crate::ec_comm;
use crate::sleep::sleep_or;

const FAILED_INPUT: &str = "Failed to get keypress";

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
    pub power_info: Option<PowerInfo>,
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
    prev_power_info: Option<Option<PowerInfo>>,
    prev_pressed: bool,
    verbose: bool,
    keypress: Option<Key>,
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

#[derive(Debug, PartialEq, Eq)]
enum StateFrame {
    NoExpansionBay(BayIssue),
    NoHubboard,
    TouchpadButNoInputmodules,
    InputModulesMissing,
    NoTouchpad,
    Finished,
}

impl State {
    pub fn new() -> Self {
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
            prev_pressed: false,
            verbose: built_info::DEBUG,
            keypress: None,
        }
    }

    // Very simple state machine
    // The next state does not depend on the current state
    fn next_state(
        &self,
        input_deck: &InputDeckStatus,
        expansion_bay: &EcResponseExpansionBayStatus,
        hw_diag: &EcResponseGetHwDiag,
    ) -> StateFrame {
        // Expected only as the first state
        // Otherwise won't usually happen because you're not supposed to hot-plug the module.
        // Can still happen as later state if the user opens the lid or hatch.
        if let Some(issue) = has_bay_issue(expansion_bay, hw_diag) {
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
            (true, true) => StateFrame::Finished,
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
            StateFrame::NoExpansionBay(bay_issue) => {
                if bay_issue == BayIssue::InterposerOpen {
                    self.bitmaps.latch_warning().draw(buffer)
                }
                self.bitmaps.expansion_bay_missing().draw(buffer)
            }
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
            _ => {
                // Button indicators right
                self.bitmaps.button_short_skip().draw(buffer);
                self.bitmaps.button_long_off().draw(buffer);
            }
        }

        // QR Codes
        match self.frame {
            StateFrame::NoExpansionBay(_) => {
                self.bitmaps.qr_code_1().draw(buffer);
            }
            StateFrame::NoHubboard => {
                self.bitmaps.qr_code_2().draw(buffer);
            }
            StateFrame::Finished => {}
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
                | StateFrame::Finished
        ) {
            self.draw_inputmodules(buffer, input_deck)
        }

        // Help text on the bottom
        let warn_x = WIDTH_I32 / 2;
        let warn_y = HEIGHT_I32 - 114 - 75;
        match self.frame {
            StateFrame::NoExpansionBay(issue) => {
                draw_textbox(
                    buffer,
                    Point::new(warn_x, warn_y),
                    TextColor::Warn,
                    &issue.to_string(),
                );
            }
            StateFrame::NoHubboard => {
                draw_textbox(
                    buffer,
                    Point::new(warn_x, warn_y),
                    TextColor::Warn,
                    "Input Module Connector Board not detected",
                );
            }
            StateFrame::TouchpadButNoInputmodules => {
                draw_textbox(
                    buffer,
                    Point::new(warn_x, warn_y),
                    TextColor::Warn,
                    "Input Module not detected",
                );
            }
            StateFrame::InputModulesMissing => {
                draw_textbox(
                    buffer,
                    Point::new(warn_x, warn_y),
                    TextColor::Warn,
                    "Input Module not detected",
                );
            }
            StateFrame::NoTouchpad => {
                draw_textbox(
                    buffer,
                    Point::new(warn_x, warn_y),
                    TextColor::Warn,
                    "Touchpad not detected",
                );
            }
            StateFrame::Finished => {
                draw_textbox(
                    buffer,
                    Point::new(warn_x, warn_y),
                    TextColor::Ok,
                    "Input Modules and Touchpad detected",
                );
            }
        }
    }

    fn draw_inputmodules(&mut self, buffer: &mut Buffer, input_deck: &InputDeckStatus) {
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
        power_info: &Option<PowerInfo>,
    ) -> bool {
        Some(input_deck) != self.prev_input_deck.as_ref()
            || Some(expansion_bay) != self.prev_expansion_bay.as_ref()
            || Some(hw_diag) != self.prev_hw_diag.as_ref()
            || Some(power_info) != self.prev_power_info.as_ref()
    }
}

fn get_gop_buffer(bs: &BootServices) -> Option<(ScopedProtocol<GraphicsOutput>, Buffer)> {
    let gop_handles = if let Ok(gop_handles) =
        bs.locate_handle_buffer(SearchType::ByProtocol(&GraphicsOutput::GUID))
    {
        gop_handles
    } else {
        info!("GraphicsOutput protocol is missing");
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
        info!("No GOP handles found");
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
            info!("Input protocol is missing");
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

pub fn start(
    bs: &BootServices,
    _st: &SystemTable<Boot>,
    ec: &Option<CrosEc>,
) -> uefi::Result<bool> {
    let mut input_deck = ec_comm::get_input_deck(ec);
    let mut expansion_bay = ec_comm::get_expansion_bay(ec);
    let mut hw_diag = ec_comm::get_hw_diag(ec);
    let mut power_info = ec.as_ref().and_then(power::power_info);

    // If we start out as fully populated we want to boot as fast as possible
    // and skip the GUI
    let state = State::new();
    if state.next_state(&input_deck, &expansion_bay, &hw_diag) == StateFrame::Finished {
        info!("Started out as fully populated, booting immediately");
        return Ok(false);
    }

    if !matches!(
        input_deck.state,
        InputDeckState::Disconnected | InputDeckState::TurningOn | InputDeckState::On
    ) {
        println!("Input Deck State Overridden: {:?}. No detection of modules possible - continuing to boot ", input_deck.state);
        bs.stall(1_000_000);
        return Ok(false);
    }

    // Get GOP protocol to draw on the screen
    let (mut gop, mut buffer) = if let Some((gop, buf)) = get_gop_buffer(bs) {
        (gop, buf)
    } else {
        // Something went wrong with GOP initialization, keep booting
        // TODO: Maybe in DVT2 we should still assert here?
        return Ok(false);
    };

    // Get Input protocol to detect keypresses
    let mut input = get_input_protocol(bs).expect("Should always have input protocol");

    let mut state = State::new();
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
            power_info = ec.as_ref().and_then(power::power_info);
        } else {
            ec_comm::keypress_mock_data(
                state.keypress,
                &mut input_deck,
                &mut expansion_bay,
                &mut hw_diag,
                &mut power_info,
            );
        }
        // TODO: Maybe mock power button press?
        let button_pressed = ec_comm::button_pressed(ec);

        // Detect short press of power button
        // Short press (or ESC) will exit and boot (skip all further checks)
        if (!button_pressed && state.prev_pressed && long_press == 1)
            || state.keypress == Some(Key::Special(ScanCode::ESCAPE))
        {
            break;
        }

        let need_redraw = state.need_redraw(&input_deck, &expansion_bay, &hw_diag, &power_info);

        let q_key = Char16::try_from('?').unwrap();
        let need_redraw = match state.keypress {
            Some(Key::Printable(key)) if key == q_key => {
                state.verbose = !state.verbose;
                true
            }
            _ => need_redraw,
        };

        // Detect long press of power button
        if button_pressed && state.prev_pressed && !need_redraw {
            if long_press > 4 {
                // Exit and signal that we need to poweroff
                return Ok(true);
            }
            // Wait a second
            sleep_or(bs, 1_000, unsafe {
                Some(input.wait_for_key_event().unsafe_clone())
            });
            long_press += 1;
        } else {
            long_press = 0;
        }

        if need_redraw {
            state.frame = state.next_state(&input_deck, &expansion_bay, &hw_diag);
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
        state.prev_pressed = button_pressed;
    }
    let _ = clear_screen(&mut buffer, &mut gop);
    Ok(false)
}
