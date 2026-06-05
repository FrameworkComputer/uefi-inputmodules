// TODO: For when I ported the font
// use alloc::string::ToString;
use alloc::format;
use core::fmt;

use embedded_graphics::geometry::Size;
use embedded_graphics::prelude::Point;

use framework_lib::chromium_ec::commands::{
    EcResponseGetHwDiag, DIAGNOSTICS_AUDIO_DAUGHTERBOARD, DIAGNOSTICS_HW_NO_BATTERY,
    DIAGNOSTICS_NOFAN, DIAGNOSTICS_TOUCHPAD,
};
use framework_lib::chromium_ec::input_deck::{InputDeckState, InputModuleType};
use framework_lib::chromium_ec::CrosEc;
use framework_lib::power::{self, get_accel_data, AccelData, LidAngle, ReducedPowerInfo};
#[allow(unused_imports)]
use log::{debug, error, info, trace};
use log::{log_enabled, Level};
use uefi::prelude::BootServices;
use uefi::proto::console::text::{Key, ScanCode};
use uefi::table::{Boot, SystemTable};
use uefi::Char16;
#[allow(unused_imports)]
use uefi_services::{print, println, system_table};

use crate::buffer::Buffer;
use crate::built_info;
use crate::drawing_12in::*;
use crate::ec_comm;
use crate::sleep::sleep_or;
use crate::util;

const FAILED_INPUT: &str = "Failed to get keypress";

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct InputDeckStatus12In {
    pub chassis_closed: bool,
    pub pwrbtn_present: bool,
    pub audioboard_present: bool,
    pub touchpad_present: bool,
}

impl InputDeckStatus12In {
    pub fn fully_populated(&self) -> bool {
        self.chassis_closed
            && self.pwrbtn_present
            && self.audioboard_present
            && self.touchpad_present
    }
}

#[allow(dead_code)]
pub struct DebugInfo {
    pub power_button: bool,
    pub ec_present: bool,
    pub long_press: i32,
    pub hw_diag: EcResponseGetHwDiag,
    pub deck: InputDeckStatus12In,
    pub power_info: Option<ReducedPowerInfo>,
    pub accel_reading: (AccelData, AccelData, LidAngle),
}

pub struct State {
    // Cache bitmaps for drawing
    bitmaps: StateBitmaps,
    screen_size: Size,
    // Currently displayed frame
    frame: StateFrame,
    prev_input_deck: Option<InputDeckStatus12In>,
    prev_hw_diag: Option<EcResponseGetHwDiag>,
    prev_power_info: Option<Option<ReducedPowerInfo>>,
    prev_accel_reading: Option<(AccelData, AccelData, LidAngle)>,
    prev_pressed: bool,
    verbose: bool,
    keypress: Option<Key>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StateFrame {
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
            prev_hw_diag: None,
            prev_power_info: None,
            prev_accel_reading: None,
            prev_pressed: false,
            verbose: built_info::DEBUG,
            keypress: None,
        }
    }

    // Very simple state machine
    // The next state does not depend on the current state
    fn next_state(
        &self,
        input_deck: &InputDeckStatus12In,
        hw_diag: &EcResponseGetHwDiag,
    ) -> StateFrame {
        // TODO: Power button daughterboard
        // TODO: Sunflower evaluate more error conditions

        // TODO: Do we need to care about audio and TP diags, when we have board ID HC?
        // I think no, because they're both safely hotpluggable
        // let audioboard_present = hw_diag.hw_diag & (1 << DIAGNOSTICS_AUDIO_DAUGHTERBOARD) == 0;
        // let touchpad_present = hw_diag.hw_diag & (1 << DIAGNOSTICS_TOUCHPAD) == 0;

        let fan_present = hw_diag.hw_diag & (1 << DIAGNOSTICS_NOFAN) == 0;
        if input_deck.fully_populated() && fan_present {
            StateFrame::Finished
        } else {
            StateFrame::NoTouchpad
        }
    }

    // Display graphics for the current frame
    // Need to call blit after the function call to show it
    pub fn draw_state(
        &mut self,
        buffer: &mut Buffer,
        input_deck: &InputDeckStatus12In,
        hw_diag: &EcResponseGetHwDiag,
    ) {
        buffer.clear(0xFF, 0xFF, 0xFF);

        self.bitmaps.bare().draw(buffer);

        // Hints on lower left and right
        match self.frame {
            StateFrame::Finished => {
                // TODO: No bitmaps yet
                // self.bitmaps.button_boot().draw(buffer);
            }
            _ => {
                // TODO: No bitmaps yet
                // self.bitmaps.button_short_skip().draw(buffer);
            }
        }

        // QR Codes
        match self.frame {
            StateFrame::Finished => {}
            _ => {
                // TODO: No bitmaps yet
                // self.bitmaps.qr_code_3().draw(buffer);
            }
        }

        // Help text on the bottom
        match self.frame {
            StateFrame::NoTouchpad => {
                // TODO: No bitmaps yet
                // self.bitmaps.touchpad_not_detected_text().draw(buffer);
                // TODO: Add camera module detection
                // EC will check touchpad board ID as well as KB I2C communication.
                // The distinction is not useful to the user, they can't do anything differently to debug it.
                let fan_present = hw_diag.hw_diag & (1 << DIAGNOSTICS_NOFAN) == 0;
                let battery_present = hw_diag.hw_diag & (1 << DIAGNOSTICS_HW_NO_BATTERY) == 0;
                let warn = format!(
                    "Issues with system installation found:{}{}{}{}{}{}\n\nPress power button to skip checks.\nHold power button for 5 seconds to shut off.",
                    if input_deck.chassis_closed { "" } else { "\n - Chassis not closed" },
                    if input_deck.touchpad_present { "" } else { "\n - Touchpad/Keyboard not properly connected" },
                    if input_deck.audioboard_present { "" } else { "\n - Audio Daughterboard not present" },
                    if input_deck.pwrbtn_present { "" } else { "\n - Power Button Daughterboard not present" },
                    if fan_present { "" } else { "\n - CPU Fan not present" },
                    if battery_present { "" } else { "\n - Battery not present" },
                );

                if !input_deck.chassis_closed {
                    self.bitmaps.switch_highlight().draw(buffer);
                }
                if !input_deck.touchpad_present {
                    self.bitmaps.pogo_highlight().draw(buffer);
                    self.bitmaps.flap_highlight().draw(buffer);
                }
                if !input_deck.audioboard_present {
                    self.bitmaps.audio_highlight().draw(buffer);
                }
                if !input_deck.pwrbtn_present {
                    self.bitmaps.pwrbtn_highlight().draw(buffer);
                }
                if !fan_present {
                    self.bitmaps.fan_highlight().draw(buffer);
                }
                if !battery_present {
                    self.bitmaps.battery_top_highlight().draw(buffer);
                    self.bitmaps.battery_bottom_highlight().draw(buffer);
                }

                draw_textbox(buffer, Point::new(WARN_X, WARN_Y), TextColor::Warn, &warn);
            }
            StateFrame::Finished => {
                // TODO: No bitmaps yet
                // self.bitmaps.inputmodules_touchpad_ok_text().draw(buffer);

                draw_textbox(
                    buffer,
                    Point::new(WARN_X, WARN_Y),
                    TextColor::Ok,
                    "Input Deck correctly installed\nPress power button to boot",
                );
            }
        }
    }

    pub fn need_redraw(
        &self,
        input_deck: &InputDeckStatus12In,
        hw_diag: &EcResponseGetHwDiag,
        power_info: &Option<ReducedPowerInfo>,
        accel_reading: &(AccelData, AccelData, LidAngle),
    ) -> bool {
        let new_deck = Some(input_deck) != self.prev_input_deck.as_ref();
        let new_diag = Some(hw_diag) != self.prev_hw_diag.as_ref();
        let new_power = Some(power_info) != self.prev_power_info.as_ref();
        let new_accel_reading = Some(accel_reading) != self.prev_accel_reading.as_ref();

        if new_deck {
            debug!("Deck Changed");
            debug!("Old: {:?}", self.prev_input_deck.as_ref());
            debug!("New: {:?}", input_deck);
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
        if new_accel_reading {
            debug!("Accel Changed");
            debug!("Old: {:?}", self.prev_accel_reading.as_ref());
            debug!("New: {:?}", accel_reading);
        }

        new_deck || new_diag || new_power || new_accel_reading
    }
}

pub fn start(
    bs: &BootServices,
    st: &SystemTable<Boot>,
    ec: &Option<CrosEc>,
    quick_boot: bool,
) -> uefi::Result<Option<uefi::table::runtime::ResetType>> {
    let mut input_deck = ec_comm::get_input_deck(ec);
    let mut hw_diag = ec_comm::get_hw_diag(ec);
    let mut accel_reading = ec_comm::get_accel_reading(ec);
    let mut power_info: Option<ReducedPowerInfo> = ec
        .as_ref()
        .and_then(power::power_info)
        .map(Into::<ReducedPowerInfo>::into);

    let state = State::new();
    // If we start out as fully populated we want to boot as fast as possible
    // and skip the GUI
    if quick_boot && state.next_state(&input_deck, &hw_diag) == StateFrame::Finished {
        info!("Started out as fully populated, booting immediately");
        return Ok(None);
    }

    // TODO: Might want this
    // if !matches!(
    //     input_deck.state,
    //     InputDeckState::Disconnected | InputDeckState::TurningOn | InputDeckState::On
    // ) {
    //     println!("Input Deck State Overridden: {:?}. No detection of modules possible - continuing to boot ", input_deck.state);
    //     bs.stall(1_000_000);
    //     return Ok(None);
    // }

    // Get GOP protocol to draw on the screen
    let (mut gop, mut buffer, original_mode) = if let Some(res) = util::get_gop_buffer(bs) {
        res
    } else {
        // Something went wrong with GOP initialization, keep booting
        // TODO: Maybe in DVT2 we should still assert here?
        return Ok(None);
    };

    // Get Input protocol to detect keypresses
    let mut input = util::get_input_protocol(bs).expect("Should always have input protocol");

    let mut state = State::new();
    let mut long_press = 0;
    loop {
        #[cfg(feature = "azalea")]
        let azalea = true;
        #[cfg(not(feature = "azalea"))]
        let azalea = false;
        if !azalea && ec.is_some() {
            input_deck = ec_comm::get_input_deck(ec);
            hw_diag = ec_comm::get_hw_diag(ec);
            accel_reading = ec_comm::get_accel_reading(ec);
            power_info = ec
                .as_ref()
                .and_then(power::power_info)
                .map(Into::<ReducedPowerInfo>::into);
        } else {
            ec_comm::keypress_mock_data(
                state.keypress,
                &mut input_deck,
                &mut hw_diag,
                &mut power_info,
            );
        }
        // TODO: Maybe mock power button press?
        let button_pressed = ec_comm::button_pressed(ec);

        // Detect short press of power button
        // Short press (or ESC) will exit and boot (skip all further checks)
        let short_press = (!button_pressed && state.prev_pressed && long_press == 1)
            || state.keypress == Some(Key::Special(ScanCode::ESCAPE));
        if let (true, _) = (short_press, state.frame) {
            // Just exit and continue
            break;
        }

        if state.keypress == Some(Key::Special(ScanCode::FUNCTION_12)) {
            let rt = st.runtime_services();
            util::set_os_indications_setupmenu(rt);
            return Ok(Some(uefi::table::runtime::ResetType::Cold));
        }

        let need_redraw = state.need_redraw(&input_deck, &hw_diag, &power_info, &accel_reading);
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
        // TODO: Why did we skip this if no need_redraw?
        // Right now on sunflower always redrawing due to g-sensor animation
        let need_redraw = if button_pressed && state.prev_pressed {
            // && !need_redraw {
            if long_press > 4 {
                // Exit and signal that we need to poweroff
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
            state.frame = state.next_state(&input_deck, &hw_diag);

            state.draw_state(&mut buffer, &input_deck, &hw_diag);

            draw_battery_info(&mut buffer, &power_info);
            if state.verbose {
                draw_accel_bubble(&mut buffer, &accel_reading.0, Point::new(400, 400));
                draw_accel_bubble(&mut buffer, &accel_reading.1, Point::new(400, 650));
                draw_lid_angle(&mut buffer, accel_reading.2, Point::new(950, 200));
            }
            draw_version(
                &mut buffer,
                Some(DebugInfo {
                    power_button: button_pressed,
                    ec_present: ec.is_some(),
                    long_press,
                    hw_diag,
                    deck: input_deck.clone(),
                    power_info: power_info.clone(),
                    accel_reading: accel_reading.clone(),
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
        state.prev_hw_diag = Some(hw_diag);
        state.prev_power_info = Some(power_info.clone());
        state.prev_accel_reading = Some(accel_reading.clone());
        state.prev_pressed = button_pressed;
    }
    let _ = clear_screen(&mut buffer, &mut gop);

    // Restore the resolution we changed when entering graphical mode
    if let Some(original_mode) = original_mode {
        let (width, height) = original_mode.info().resolution();
        info!("Restoring original resolution: {}x{}", width, height);
        let _ = gop.set_mode(&original_mode);
    }

    Ok(None)
}
