//! Platform specific code
//!
//! Each supported laptop lives in its own submodule and implements [`Platform`].
//! The UI loop in [`crate::app`] is shared and only calls into the platform
//! to read the hardware state, decide what to show and draw it.
//!
//! To add a new platform:
//! 1. Create `platform/<name>/mod.rs` with a type that implements [`Platform`]
//! 2. Point [`Current`] at it

use framework_lib::chromium_ec::CrosEc;
use uefi::proto::console::text::Key;

use crate::buffer::Buffer;

pub mod laptop16;

/// The platform this binary is built for
pub type Current = laptop16::Laptop16;

/// Result of [`Platform::tick`]
pub enum Tick {
    /// Keep running the UI loop
    Continue {
        /// Redraw even if nothing in the snapshot changed (e.g. a countdown)
        force_redraw: bool,
    },
    /// Leave the UI and continue booting
    Boot,
}

/// Information from the shared UI loop, for the platform to display
pub struct LoopInfo {
    pub ec_present: bool,
    pub power_button: bool,
    pub long_press: i32,
}

pub trait Platform: Sized {
    /// Everything read from the hardware in one iteration of the UI loop
    type Snapshot: Clone;

    /// Should the UI be shown at all? (e.g. based on BIOS settings)
    ///
    /// If false, the app exits immediately and the system continues booting.
    fn show_ui() -> bool {
        true
    }

    /// Print the hardware status to the console (the `info` shell command)
    fn print_status();

    fn new(ec: &Option<CrosEc>) -> Self;

    /// Read the current hardware state
    fn read(&self, ec: &Option<CrosEc>) -> Self::Snapshot;

    /// Update the snapshot from mock keypresses, used when there's no EC (QEMU)
    fn mock(&mut self, keypress: Option<Key>, snapshot: &mut Self::Snapshot);

    /// Is everything OK, so that we can boot without showing the UI?
    fn boot_immediately(&self, snapshot: &Self::Snapshot) -> bool;

    /// Called at the start of every loop iteration, before reading the hardware
    fn tick(&mut self) -> Tick {
        Tick::Continue {
            force_redraw: false,
        }
    }

    /// Handle platform specific keys
    ///
    /// Return true if the key was handled and the screen needs to be redrawn.
    fn handle_key(&mut self, _key: Key) -> bool {
        false
    }

    /// Called when the power button was short-pressed, just before continuing to boot
    fn on_short_press(&mut self) {}

    /// Did anything change that needs a redraw?
    fn need_redraw(&self, prev: Option<&Self::Snapshot>, cur: &Self::Snapshot) -> bool;

    /// Update the displayed state and draw it into the buffer
    ///
    /// The caller blits the buffer to the screen afterwards.
    fn draw(
        &mut self,
        buffer: &mut Buffer,
        prev: Option<&Self::Snapshot>,
        cur: &Self::Snapshot,
        info: &LoopInfo,
        verbose: bool,
    );
}
