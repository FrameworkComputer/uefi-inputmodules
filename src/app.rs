//! UI loop shared by all platforms
//!
//! Handles everything that's not platform specific:
//! power button short/long press, ESC, F12 (BIOS setup), '?' (verbose)
//! and only redrawing when something changed.
use framework_lib::chromium_ec::CrosEc;
#[allow(unused_imports)]
use log::{debug, error, info, trace};
use log::{log_enabled, Level};
use uefi::boot;
use uefi::proto::console::text::{Key, ScanCode};
use uefi::Char16;

use crate::built_info;
use crate::ec_comm;
use crate::platform::{LoopInfo, Platform, Tick};
use crate::sleep::sleep_or;
use crate::ui::clear_screen;
use crate::util;

const FAILED_INPUT: &str = "Failed to get keypress";

/// Run the UI until the user (or the platform) decides to continue booting
///
/// Returns the reset type, if the system should be reset/shut down instead
/// of continuing to boot.
pub fn run<P: Platform>(
    ec: &Option<CrosEc>,
    force_ui: bool,
) -> uefi::Result<Option<uefi::runtime::ResetType>> {
    let mut platform = P::new(ec);
    let mut snapshot = platform.read(ec);

    // If we start out as fully populated we want to boot as fast as possible
    // and skip the GUI
    if !force_ui && platform.boot_immediately(&snapshot) {
        info!("Started out as fully populated, booting immediately");
        return Ok(None);
    }

    // Get GOP protocol to draw on the screen
    let (mut gop, mut buffer) = if let Some((gop, buf)) = util::get_gop_buffer() {
        (gop, buf)
    } else {
        error!("inputmodules.efi - Couldn't draw to display");
        // Something went wrong with GOP initialization, keep booting
        // TODO: Maybe in DVT2 we should still assert here?
        return Ok(None);
    };

    // Get Input protocol to detect keypresses
    let mut input = util::get_input_protocol().expect("Should always have input protocol");

    // Without EC, or if the EC doesn't support our commands, use mock data
    let mock = cfg!(feature = "azalea") || ec.is_none();

    let mut prev: Option<P::Snapshot> = None;
    let mut prev_pressed = false;
    let mut keypress: Option<Key> = None;
    let mut verbose = built_info::DEBUG;
    let mut long_press = 0;
    // Tick::Boot means continue booting, e.g. after a timeout
    while let Tick::Continue {
        force_redraw: force_tick_redraw,
    } = platform.tick()
    {
        if mock {
            platform.mock(keypress, &mut snapshot);
        } else {
            snapshot = platform.read(ec);
        }
        // TODO: Maybe mock power button press?
        let button_pressed = ec_comm::button_pressed(ec);

        // Detect short press of power button
        // Short press (or ESC) will exit and boot (skip all further checks)
        let short_press =
            (!button_pressed && prev_pressed) || keypress == Some(Key::Special(ScanCode::ESCAPE));
        if short_press {
            platform.on_short_press();
            break;
        }

        if keypress == Some(Key::Special(ScanCode::FUNCTION_12)) {
            util::set_os_indications_setupmenu();
            info!("Rebooting into BIOS Setup Menu");
            return Ok(Some(uefi::runtime::ResetType::COLD));
        }

        // If verbose always need to update, to update things like fan info in the debug section
        let need_redraw =
            force_tick_redraw || verbose || platform.need_redraw(prev.as_ref(), &snapshot);
        if need_redraw && log_enabled!(Level::Debug) {
            // Sleep after need_redraw prints debug information
            boot::stall(core::time::Duration::from_micros(1_000_000));
        }

        let q_key = Char16::try_from('?').unwrap();
        let need_redraw = match keypress {
            Some(Key::Printable(key)) if key == q_key => {
                verbose = !verbose;
                true
            }
            Some(key) if platform.handle_key(key) => true,
            _ => need_redraw,
        };

        // Detect long press of power button
        let need_redraw = if button_pressed && prev_pressed && !need_redraw {
            if long_press > 4 {
                // Exit and signal that we need to poweroff
                info!("Powering off");
                return Ok(Some(uefi::runtime::ResetType::SHUTDOWN));
            }
            // Wait a second
            sleep_or(1_000, input.wait_for_key_event().ok());
            long_press += 1;
            true
        } else {
            long_press = 0;
            need_redraw
        };

        if need_redraw {
            let info = LoopInfo {
                ec_present: ec.is_some(),
                power_button: button_pressed,
                long_press,
            };
            platform.draw(&mut buffer, prev.as_ref(), &snapshot, &info, verbose);
            buffer.blit(&mut gop)?;
        }

        // TODO: It needs to be very low to detect short power button presses,
        // but it doesn't have to be that low for the input modules. Maybe we can find a compromise
        // Put CPU to sleep until key press or 10ms timeout
        sleep_or(10, input.wait_for_key_event().ok());
        keypress = input.read_key().expect(FAILED_INPUT);

        // Save current values to access and compare them in next iteration
        prev = Some(snapshot.clone());
        prev_pressed = button_pressed;
    }
    let _ = clear_screen(&mut buffer, &mut gop);
    info!("Exited loop, continuing to boot");
    Ok(None)
}
