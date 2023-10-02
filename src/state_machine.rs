use embedded_graphics::geometry::Size;
use embedded_graphics::prelude::Point;

use framework_lib::chromium_ec::commands::{EcResponseExpansionBayStatus, EcResponseGetHwDiag};
use framework_lib::chromium_ec::input_deck::InputDeckStatus;
use framework_lib::chromium_ec::input_deck::{InputDeckState, InputModuleType};
use framework_lib::chromium_ec::CrosEc;
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
    pub gpu0: u8,
    pub gpu1: u8,
    pub bay_enabled: bool,
    pub bay_fault: bool,
    pub hatch_closed: bool,
    pub hw_diag: u32,
    pub deck: InputDeckStatus,
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
    prev_pressed: bool,
    verbose: bool,
    keypress: Option<Key>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum StateFrame {
    NoExpansionBay,
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
            prev_pressed: false,
            verbose: built_info::DEBUG,
            keypress: None,
        }
    }

    // Very simple state machine
    // The next state does not depend on the current state
    pub fn next_state(
        &self,
        input_deck: &InputDeckStatus,
        expansion_bay: &EcResponseExpansionBayStatus,
        hw_diag: &EcResponseGetHwDiag,
    ) -> StateFrame {
        // Expected only as the first state
        // Otherwise won't usually happen because you're not supposed to hot-plug the module.
        // Can still happen as later state if the user opens the lid or hatch.
        if !bay_ok(expansion_bay, hw_diag) {
            return StateFrame::NoExpansionBay;
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
        // Draw solid background color #567676
        for y in 0..self.screen_size.height {
            for x in 0..self.screen_size.width {
                // TODO: Can actually unwrap because the screen size should be correct.
                if let Some(pixel) = buffer.pixel(x as usize, y as usize) {
                    pixel.red = 0x56;
                    pixel.green = 0x76;
                    pixel.blue = 0x76;
                }
            }
        }

        // Hints on lower left and right
        // Draw laptop with latches either closed or open
        match self.frame {
            StateFrame::Finished => {
                // Boot Button indicator right
                self.bitmaps.button_boot().draw(buffer);

                // Draw laptop with latches closed
                self.bitmaps.laptop_latches_closed().draw(buffer);
            }
            _ => {
                // Step by step QR Code left
                // TODO: Add QR codes when they're ready
                _ = self.bitmaps.qr_code_left();
                //self.bitmaps.qr_code_left().draw(buffer);

                // Button indicators right
                self.bitmaps.button_short_skip().draw(buffer);
                self.bitmaps.button_long_off().draw(buffer);

                // Draw laptop with latches open
                self.bitmaps.laptop_latches_open().draw(buffer);
                // Fix up the laptop
                if self.frame != StateFrame::NoExpansionBay {
                    self.bitmaps.expansion_bay_populated().draw(buffer)
                }
                if self.frame == StateFrame::NoHubboard {
                    self.bitmaps.hubboard_missing().draw(buffer)
                }
            }
        }

        // Instruction images
        match self.frame {
            StateFrame::NoExpansionBay => {
                self.bitmaps.no_expansion_bay_instr().draw(buffer);
            }
            StateFrame::NoHubboard => {
                self.bitmaps.no_hubboard_instr().draw(buffer);
            }
            StateFrame::TouchpadButNoInputmodules => {
                self.bitmaps.touchpad_no_modules_instr().draw(buffer);
            }
            StateFrame::InputModulesMissing => {
                self.bitmaps.inputmodules_missing_instr().draw(buffer);
            }
            StateFrame::NoTouchpad => {
                self.bitmaps.no_touchpad_instr().draw(buffer);
            }
            StateFrame::Finished => {}
        }

        // Latch help
        match self.frame {
            StateFrame::TouchpadButNoInputmodules => {
                self.bitmaps.latch_open_help_l().draw(buffer);
                self.bitmaps.latch_open_help_r().draw(buffer);

                draw_textbox(
                    buffer,
                    Point::new(WIDTH_I32 - 218 - 761, 1070),
                    TextColor::Info,
                    "1. Unlatch the Latches",
                );
                draw_textbox(
                    buffer,
                    Point::new(WIDTH_I32 - 218 - 761, 1150),
                    TextColor::Info,
                    "2. Unlatch the Latches",
                );
            }
            StateFrame::Finished => {
                self.bitmaps.latch_close_help_l().draw(buffer);
                self.bitmaps.latch_close_help_r().draw(buffer);

                draw_textbox(
                    buffer,
                    Point::new(1555, 1100),
                    TextColor::Info,
                    "Press in latch to lock",
                );
            }
            _ => {}
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
        match self.frame {
            StateFrame::NoExpansionBay => {
                draw_textbox(
                    buffer,
                    Point::new(620, HEIGHT_I32 - 96 - 75),
                    TextColor::Warn,
                    "Expansion Bay Module or Expansion Bay Cable not detected",
                );
            }
            StateFrame::NoHubboard => {
                draw_textbox(
                    buffer,
                    Point::new(620, HEIGHT_I32 - 96 - 75),
                    TextColor::Warn,
                    "Input Module Connector Board not detected",
                );
            }
            StateFrame::TouchpadButNoInputmodules => {
                draw_textbox(
                    buffer,
                    Point::new(620, HEIGHT_I32 - 96 - 75),
                    TextColor::Warn,
                    "Input Module not detected",
                );
            }
            StateFrame::InputModulesMissing => {
                draw_textbox(
                    buffer,
                    Point::new(620, HEIGHT_I32 - 96 - 75),
                    TextColor::Warn,
                    "Input Module not detected",
                );
            }
            StateFrame::NoTouchpad => {
                draw_textbox(
                    buffer,
                    Point::new(620, HEIGHT_I32 - 96 - 75),
                    TextColor::Warn,
                    "Touchpad not detected",
                );
            }
            StateFrame::Finished => {
                draw_textbox(
                    buffer,
                    Point::new(620, HEIGHT_I32 - 96 - 75),
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
                    x: 329 + 98 - 1,
                    y: HEIGHT_I32 - 263 - 263 - 319,
                },
            );
        }
        if input_deck.top_row.pos3 == InputModuleType::GenericC {
            self.bitmaps.c1_module().draw_pos(
                buffer,
                Point {
                    x: 329 + 804 - 2,
                    y: HEIGHT_I32 - 263 - 263 - 319,
                },
            );
        }
        if input_deck.top_row.pos4 == InputModuleType::GenericC {
            self.bitmaps.c1_module().draw_pos(
                buffer,
                Point {
                    x: 329 + 804 + 98 - 3,
                    y: HEIGHT_I32 - 263 - 263 - 319,
                },
            );
        }

        if input_deck.top_row.pos0 == InputModuleType::GenericA
            || input_deck.top_row.pos0 == InputModuleType::KeyboardA
        {
            self.bitmaps.keyboard().draw_pos(
                buffer,
                Point {
                    x: 329,
                    y: HEIGHT_I32 - 263 - 263 - 319,
                },
            );
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
                    x: 329 + 97 * 2 - 1,
                    y: HEIGHT_I32 - 263 - 263 - 319,
                },
            );
        }

        if input_deck.top_row.pos0 == InputModuleType::GenericB
            || input_deck.top_row.pos0 == InputModuleType::KeyboardB
        {
            // TODO: Numpad image
            self.bitmaps.b1_module().draw(buffer);
        }
        if input_deck.top_row.pos3 == InputModuleType::GenericB
            || input_deck.top_row.pos3 == InputModuleType::KeyboardB
        {
            // TODO: Numpad image
            self.bitmaps.b1_module().draw_pos(
                buffer,
                Point {
                    x: 329 + 804 - 2,
                    y: HEIGHT_I32 - 263 - 263 - 319,
                },
            );
        }

        if input_deck.touchpad_present {
            self.bitmaps.touchpad().draw(buffer);
        }
    }

    pub fn need_redraw(
        &self,
        input_deck: &InputDeckStatus,
        expansion_bay: &EcResponseExpansionBayStatus,
        hw_diag: &EcResponseGetHwDiag,
    ) -> bool {
        Some(input_deck) != self.prev_input_deck.as_ref()
            || Some(expansion_bay) != self.prev_expansion_bay.as_ref()
            || Some(hw_diag) != self.prev_hw_diag.as_ref()
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

fn bay_ok(expansion_bay: &EcResponseExpansionBayStatus, hw_diag: &EcResponseGetHwDiag) -> bool {
    // TODO: In DVT1 the UMA fan module does not have board ID resistors, so we can't detect it
    // Currently we just have to rely on the fans.
    // In DVT2 we can add this check.
    //if !expansion_bay.expansion_bay_board().is_some() && hw_diag.fan_fault() == (false, false) {
    hw_diag.fan_fault() == (false, false) && expansion_bay.hatch_switch_closed()
}

pub fn start(
    bs: &BootServices,
    _st: &SystemTable<Boot>,
    ec: &Option<CrosEc>,
) -> uefi::Result<bool> {
    let mut input_deck = ec_comm::get_input_deck(ec);
    let mut expansion_bay = ec_comm::get_expansion_bay(ec);
    let mut hw_diag = ec_comm::get_hw_diag(ec);

    // If we start out as fully populated we want to boot as fast as possible
    // and skip the GUI
    if input_deck.fully_populated() && bay_ok(&expansion_bay, &hw_diag) {
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
        } else {
            ec_comm::keypress_mock_data(
                state.keypress,
                &mut input_deck,
                &mut expansion_bay,
                &mut hw_diag,
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

        let need_redraw = state.need_redraw(&input_deck, &expansion_bay, &hw_diag);

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
            draw_version(
                &mut buffer,
                Some(DebugInfo {
                    power_button: button_pressed,
                    ec_present: ec.is_some(),
                    long_press,
                    gpu0: expansion_bay.board_id_0,
                    gpu1: expansion_bay.board_id_1,
                    bay_state: expansion_bay.state,
                    bay_enabled: expansion_bay.module_enabled(),
                    bay_fault: expansion_bay.module_fault(),
                    hatch_closed: expansion_bay.hatch_switch_closed(),
                    hw_diag: hw_diag.hw_diag,
                    deck: input_deck.clone(),
                }),
                state.verbose,
            );
            buffer.blit(&mut gop)?;
        }

        // Put CPU to sleep until key press or 10ms timeout
        sleep_or(bs, 10, unsafe {
            Some(input.wait_for_key_event().unsafe_clone())
        });
        state.keypress = input.read_key().expect(FAILED_INPUT);

        // Save current values to access and compare them in next iteration
        state.prev_input_deck = Some(input_deck.clone());
        state.prev_expansion_bay = Some(expansion_bay);
        state.prev_hw_diag = Some(hw_diag);
        state.prev_pressed = button_pressed;
    }
    let _ = clear_screen(&mut buffer, &mut gop);
    Ok(false)
}
