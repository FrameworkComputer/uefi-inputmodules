use embedded_graphics::geometry::Size;
use embedded_graphics::prelude::Point;

use framework_lib::chromium_ec::commands::{
    EcRequestExpansionBayStatus, EcResponseExpansionBayStatus, ExpansionByStates,
};
use framework_lib::chromium_ec::input_deck::InputDeckStatus;
use framework_lib::chromium_ec::input_deck::{InputDeckState, InputModuleType, TopRowPositions};
use framework_lib::chromium_ec::{CrosEc, EcRequestRaw};
#[allow(unused_imports)]
use log::{debug, error, info, trace};
use uefi::prelude::BootServices;
use uefi::proto::console::gop::GraphicsOutput;
use uefi::table::boot::{OpenProtocolAttributes, OpenProtocolParams, ScopedProtocol, SearchType};
use uefi::table::{Boot, SystemTable};
use uefi::Identify;

use crate::buffer::Buffer;
use crate::drawing::*;

pub struct State {
    bitmaps: StateBitmaps,
    screen_size: Size,
    frame: StateFrame,
    prev_input_deck: Option<InputDeckStatus>,
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
                let pixel = buffer.pixel(x as usize, y as usize).unwrap();
                pixel.red = 0x56;
                pixel.green = 0x76;
                pixel.blue = 0x76;
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
        state: (ExpansionByStates::ModuleEnabled as u8)
            | (ExpansionByStates::HatchSwitchClosed as u8),
        board_id_0: 0,
        board_id_1: 0,
    };

    #[cfg(not(feature = "qemu"))]
    let expansion_bay = EcRequestExpansionBayStatus {}.send_command(ec.as_ref().unwrap());
    #[cfg(feature = "qemu")]
    let expansion_bay: core::result::Result<EcResponseExpansionBayStatus, ()> =
        core::result::Result::Err(());
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

    #[cfg(not(feature = "qemu"))]
    let input_deck = ec.as_ref().unwrap().get_input_deck_status();
    #[cfg(feature = "qemu")]
    let input_deck: core::result::Result<InputDeckStatus, ()> = core::result::Result::Err(());
    if let Ok(input_deck) = input_deck {
        //println!("Got it!");
        input_deck
    } else {
        //println!("Emulated deck data");
        emulated_deck
    }
}

pub fn start(bs: &BootServices, _st: &SystemTable<Boot>, ec: &Option<CrosEc>) -> uefi::Result {
    let input_deck = get_input_deck(ec);
    let expansion_bay = get_expansion_bay(ec);

    // If we start out as fully populated we want to boot as fast as possible
    // and skip the GUI
    if input_deck.fully_populated() && expansion_bay.module_enabled() {
        info!("Started out as fully populated, booting immediately");
        return Ok(());
    }

    let (mut gop, mut buffer) = if let Some((gop, buf)) = get_gop_buffer(bs) {
        (gop, buf)
    } else {
        // Something went wrong with GOP initialization, keep booting
        // TODO: Maybe in DVT2 we should still assert here?
        return Ok(());
    };

    let mut state = State::new();
    loop {
        let input_deck = get_input_deck(ec);
        let expansion_bay = get_expansion_bay(ec);

        let _need_redraw = Some(&input_deck) != state.prev_input_deck.as_ref();
        state.frame = state.next_state(&input_deck, &expansion_bay);

        state.draw_state(&mut buffer, &input_deck);
        draw_version(&mut buffer);
        buffer.blit(&mut gop)?;
        bs.stall(2_000_000);

        state.prev_input_deck = Some(input_deck);
    }
    clear_screen(&mut buffer, &mut gop);
    Ok(())
}
