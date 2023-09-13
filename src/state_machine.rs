use embedded_graphics::geometry::Size;
use embedded_graphics::prelude::Point;

use alloc::string::ToString;
use framework_lib::chromium_ec::commands::{
    EcRequestExpansionBayStatus, EcRequestUefiAppBtnStatus, EcRequestUefiAppMode,
    EcResponseExpansionBayStatus, ExpansionByStates,
};
use framework_lib::chromium_ec::input_deck::{self, InputDeckStatus};
use framework_lib::chromium_ec::input_deck::{InputDeckState, InputModuleType, TopRowPositions};
use framework_lib::chromium_ec::{CrosEc, EcError, EcRequestRaw};
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
use crate::drawing::*;
use crate::sleep::sleep_or;

#[allow(dead_code)]
#[derive(Debug)]
pub struct DebugInfo {
    power_button: bool,
    ec_present: bool,
    long_press: i32,
}

pub struct State {
    bitmaps: StateBitmaps,
    screen_size: Size,
    frame: StateFrame,
    prev_input_deck: Option<InputDeckStatus>,
    prev_expansion_bay: Option<EcResponseExpansionBayStatus>,
    prev_pressed: bool,
    keypress: Option<Key>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum StateFrame {
    // If: No ExpansionBay Module
    // Short: Skip all, Long: Power Off
    // Extra Info: None, Maybe Expansion Bay ID
    NoExpansionBay,
    // If: No Hubboard
    // Short: Skip all, Long: Power Off
    // Extra Info: None
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
            //frame: StateFrame::NoExpansionBay,
            //frame: StateFrame::NoHubboard,
            //frame: StateFrame::InputModulesMissing,
            //frame: StateFrame::NoTouchpad,
            //frame: StateFrame::TouchpadButNoInputmodules,
            frame: StateFrame::Finished,
            prev_input_deck: None,
            prev_expansion_bay: None,
            prev_pressed: false,
            keypress: None,
        }
    }
    // Very simple state machine
    // The next state does not depend on the current state
    pub fn next_state(
        &self,
        input_deck: &InputDeckStatus,
        expansion_bay: &EcResponseExpansionBayStatus,
    ) -> StateFrame {
        // Expected only as the first state
        // Otherwise won't usually happen because you're not supposed to hot-plug the module.
        // Can still happen as later state if the user opens the lid or hatch.
        if !expansion_bay.module_enabled() {
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
                // Button indicator right
                self.bitmaps.button_short_skip().draw(buffer);

                // Draw laptop with latches closed
                self.bitmaps.laptop_latches_closed().draw(buffer);
            }
            _ => {
                // Step by step QR Code left
                self.bitmaps.qr_code_left().draw(buffer);

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
    ) -> bool {
        Some(input_deck) != self.prev_input_deck.as_ref()
            || Some(expansion_bay) != self.prev_expansion_bay.as_ref()
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

pub fn get_expansion_bay(ec: &Option<CrosEc>) -> EcResponseExpansionBayStatus {
    let emulated_bay = EcResponseExpansionBayStatus {
        state: 0,
        board_id_0: 0,
        board_id_1: 0,
    };

    let ec = ec.as_ref().ok_or(EcError::DeviceError("No EC".to_string()));
    #[cfg(not(feature = "azalea"))]
    let expansion_bay = ec.and_then(|ec| EcRequestExpansionBayStatus {}.send_command(&ec));
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

fn button_pressed(ec: &Option<CrosEc>) -> bool {
    if let Some(ref ec) = ec {
        let btn_status = EcRequestUefiAppBtnStatus {}.send_command(ec).unwrap();
        btn_status.status == 0
    } else {
        false
    }
}

pub fn set_uefi_app_mode(ec: &Option<CrosEc>, mode: bool) {
    let enable = if mode { 1 } else { 0 };
    if let Some(ref ec) = ec {
        EcRequestUefiAppMode { enable }.send_command(ec).unwrap();
    }
}

fn keypress_mock_data(
    keypress: Option<Key>,
    input_deck: &mut InputDeckStatus,
    expansion_bay: &mut EcResponseExpansionBayStatus,
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

    match keypress {
        // State: NoExpansionBay
        Some(Key::Printable(key)) if key == zero_key => {
            *expansion_bay = EcResponseExpansionBayStatus {
                state: 0,
                board_id_0: 0,
                board_id_1: 0,
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

pub fn start(
    bs: &BootServices,
    _st: &SystemTable<Boot>,
    ec: &Option<CrosEc>,
) -> uefi::Result<bool> {
    let mut input_deck = get_input_deck(ec);
    let mut expansion_bay = get_expansion_bay(ec);

    // If we start out as fully populated we want to boot as fast as possible
    // and skip the GUI
    if input_deck.fully_populated() && expansion_bay.module_enabled() {
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
    let input_handle = bs.get_handle_for_protocol::<Input>().unwrap();
    let mut input = bs.open_protocol_exclusive::<Input>(input_handle).unwrap();

    let mut state = State::new();
    let mut long_press = 0;
    loop {
        #[cfg(feature = "azalea")]
        let azalea = true;
        #[cfg(not(feature = "azalea"))]
        let azalea = false;
        if !azalea && ec.is_some() {
            input_deck = get_input_deck(ec);
            expansion_bay = get_expansion_bay(ec);
        } else {
            keypress_mock_data(state.keypress, &mut input_deck, &mut expansion_bay);
        }
        // TODO: Maybe mock power button press?
        let button_pressed = button_pressed(ec);

        // Detect short press of power button
        // Short press (or ESC) will exit and boot (skip all further checks)
        if (!button_pressed && state.prev_pressed && long_press == 1)
            || state.keypress == Some(Key::Special(ScanCode::ESCAPE))
        {
            break;
        }

        let need_redraw = state.need_redraw(&input_deck, &expansion_bay);

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
            state.frame = state.next_state(&input_deck, &expansion_bay);
            state.draw_state(&mut buffer, &input_deck);
            draw_version(&mut buffer, None);
            buffer.blit(&mut gop)?;
        }

        #[cfg(feature = "azalea")]
        draw_version(
            &mut buffer,
            Some(DebugInfo {
                power_button: button_pressed,
                ec_present: ec.is_some(),
                long_press,
            }),
        );
        #[cfg(feature = "azalea")]
        buffer.blit(&mut gop)?;

        // Put CPU to sleep until key press or 10ms timeout
        sleep_or(bs, 10, unsafe {
            Some(input.wait_for_key_event().unsafe_clone())
        });
        state.keypress = input.read_key().unwrap();

        state.prev_input_deck = Some(input_deck.clone());
        state.prev_expansion_bay = Some(expansion_bay.clone());
        state.prev_pressed = button_pressed;
    }
    let _ = clear_screen(&mut buffer, &mut gop);
    Ok(false)
}
