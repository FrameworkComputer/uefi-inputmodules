use core::mem::size_of;
use core::slice;
#[allow(unused_imports)]
use log::{debug, error};
use uefi::table::runtime::{ResetType, RuntimeServices, VariableAttributes, VariableVendor};
use uefi::{cstr16, guid, CStr16, Status};

use crate::built_info;

/// Bump whenever the layout of [`RunRecord`] changes, so the OS side knows
/// which fields it can trust
pub const RECORD_VERSION: u8 = 1;

/// TODO: Replace with our official OEM variable GUID
pub const VENDOR: VariableVendor = VariableVendor(guid!("0653cceb-295a-42ce-8c88-a00f56e3fed8"));

pub const VARIABLE_NAME: &CStr16 = cstr16!("InputmodulesAppRun");

/// Written once when we start and once when we're done
pub mod flags {
    /// We reached the end of main. Without it we crashed, hung, or the user
    /// powered off while the UI was up
    pub const FINISHED: u32 = 1 << 0;
    /// Launched with an argument that only prints info, so no UI is expected
    pub const SHELL_INFO_MODE: u32 = 1 << 1;
    /// Launched with 'force'
    pub const FORCE_UI: u32 = 1 << 2;
    /// EFI_OEM_VARIABLE_AREA_PROTOCOL was missing, we don't know the BIOS settings
    pub const NO_BIOS_SETTINGS: u32 = 1 << 3;
    /// Standalone mode or the inputmodule setting told us not to show the UI
    pub const SKIPPED_BY_BIOS_SETTING: u32 = 1 << 4;
    /// Everything was already fine, so we skipped the UI to boot as fast as possible
    pub const SKIPPED_ALL_OK: u32 = 1 << 5;
    /// We wanted to show the UI but couldn't get a framebuffer
    pub const NO_GOP: u32 = 1 << 6;
    /// We got a framebuffer and entered the UI loop
    pub const UI_SHOWN: u32 = 1 << 7;
    /// duration_us is valid. Not set if EFI_TIMESTAMP_PROTOCOL is missing
    pub const HAVE_DURATION: u32 = 1 << 8;
    /// Debug build, so timings are not representative
    pub const DEBUG_BUILD: u32 = 1 << 9;
    /// We asked for a reset instead of continuing to boot, see reset_type
    pub const RESET_REQUESTED: u32 = 1 << 10;
}

/// Every issue we showed the user, accumulated over the whole run
pub mod issues {
    pub const NO_EXPANSION_BAY: u32 = 1 << 0;
    pub const INTERPOSER_CLOSED: u32 = 1 << 1;
    pub const NO_HUBBOARD: u32 = 1 << 2;
    pub const TOUCHPAD_BUT_NO_INPUTMODULES: u32 = 1 << 3;
    pub const INPUT_MODULES_MISSING: u32 = 1 << 4;
    pub const NO_TOUCHPAD: u32 = 1 << 5;
    pub const OUTDATED_FIRMWARE: u32 = 1 << 6;
    pub const CHECKING_DIAGNOSTICS: u32 = 1 << 7;

    /// Which BayIssue came with NO_EXPANSION_BAY
    pub const BAY_LEFT_FAN: u32 = 1 << 8;
    pub const BAY_RIGHT_FAN: u32 = 1 << 9;
    pub const BAY_BOTH_FANS: u32 = 1 << 10;
    pub const BAY_INTERPOSER_OPEN: u32 = 1 << 11;
    pub const BAY_MODULE_MISSING: u32 = 1 << 12;
    pub const BAY_BAD_CONNECTION: u32 = 1 << 13;

    /// Which firmware came with OUTDATED_FIRMWARE
    pub const OUTDATED_KEYBOARD: u32 = 1 << 14;
    pub const OUTDATED_PD1: u32 = 1 << 15;
    pub const OUTDATED_PD2: u32 = 1 << 16;
    pub const OUTDATED_PD3: u32 = 1 << 17;
}

/// Which StateFrame we were in last, see [`RunRecord::final_frame`]
pub mod frame {
    /// Never entered the UI loop
    pub const NONE: u8 = 0;
    pub const NO_EXPANSION_BAY: u8 = 1;
    pub const INTERPOSER_CLOSED: u8 = 2;
    pub const NO_HUBBOARD: u8 = 3;
    pub const TOUCHPAD_BUT_NO_INPUTMODULES: u8 = 4;
    pub const INPUT_MODULES_MISSING: u8 = 5;
    pub const NO_TOUCHPAD: u8 = 6;
    pub const OUTDATED_FIRMWARE: u8 = 7;
    pub const CHECKING_DIAGNOSTICS: u8 = 8;
    pub const FINISHED: u8 = 9;
}

/// We didn't run far enough to know, or there is nothing to report
pub const UNKNOWN: u8 = 0xff;

/// What the app did during this boot, saved to a UEFI variable
///
/// Little endian, x86 only, and only ever read back on the same machine,
/// so no conversion needed.
#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
pub struct RunRecord {
    /// [`RECORD_VERSION`]
    pub version: u8,
    /// Our version from Cargo.toml as major, minor, patch
    pub app_version: [u8; 3],
    /// First 8 characters of the git commit hash, to tell builds apart.
    /// All zero if unknown
    pub git_commit: [u8; 8],
    /// See [`flags`]
    pub flags: u32,
    /// See [`issues`]
    pub issues: u32,
    /// See [`frame`]
    pub final_frame: u8,
    /// Standalone mode BIOS setting, raw value. [`UNKNOWN`] if unreadable
    pub bios_standalone_mode: u8,
    /// Inputmodule request BIOS setting, raw value. [`UNKNOWN`] if unreadable
    pub bios_inputmodule_setting: u8,
    /// EFI_RESET_TYPE we asked for. [`UNKNOWN`] if we just continued booting
    pub reset_type: u8,
    /// How long we ran. Only valid with [`flags::HAVE_DURATION`]
    pub duration_us: u64,
    /// The EFI_STATUS we returned to the boot manager
    pub exit_status: u64,
}

impl RunRecord {
    pub fn new() -> Self {
        let mut git_commit = [0u8; 8];
        if let Some(hash) = built_info::GIT_COMMIT_HASH {
            for (dst, src) in git_commit.iter_mut().zip(hash.as_bytes()) {
                *dst = *src;
            }
        }

        let flags = if built_info::DEBUG {
            flags::DEBUG_BUILD
        } else {
            0
        };

        Self {
            version: RECORD_VERSION,
            app_version: [
                parse_version_part(built_info::PKG_VERSION_MAJOR),
                parse_version_part(built_info::PKG_VERSION_MINOR),
                parse_version_part(built_info::PKG_VERSION_PATCH),
            ],
            git_commit,
            flags,
            issues: 0,
            final_frame: frame::NONE,
            bios_standalone_mode: UNKNOWN,
            bios_inputmodule_setting: UNKNOWN,
            reset_type: UNKNOWN,
            duration_us: 0,
            exit_status: 0,
        }
    }

    pub fn set(&mut self, flags: u32) {
        self.flags |= flags;
    }

    pub fn add_issues(&mut self, issues: u32) {
        self.issues |= issues;
    }

    pub fn set_reset_type(&mut self, reset_type: ResetType) {
        self.set(flags::RESET_REQUESTED);
        self.reset_type = reset_type as u8;
    }

    pub fn set_exit_status(&mut self, status: Status) {
        self.exit_status = status.0 as u64;
    }

    /// Save to the UEFI variable, overwriting whatever we saved before
    ///
    /// Deliberately volatile. The variable is gone after a reset, so if the OS
    /// can read it, the app ran during the current boot. That also means we
    /// don't wear out the SPI flash or slow down every single boot.
    pub fn write(&self, rt: &RuntimeServices) {
        // repr(C, packed), so there is no padding to leak
        let bytes =
            unsafe { slice::from_raw_parts(self as *const Self as *const u8, size_of::<Self>()) };

        let attrs = VariableAttributes::BOOTSERVICE_ACCESS | VariableAttributes::RUNTIME_ACCESS;
        match rt.set_variable(VARIABLE_NAME, &VENDOR, attrs, bytes) {
            Ok(()) => debug!("Saved run record: {:?}", self),
            // Not worth failing the boot over
            Err(err) => error!("Failed to save run record: {:?}", err),
        }
    }
}

impl Default for RunRecord {
    fn default() -> Self {
        Self::new()
    }
}

/// built gives us the version as strings, we only want to save a byte each
fn parse_version_part(part: &str) -> u8 {
    let mut version = 0u8;
    for digit in part.as_bytes() {
        if !digit.is_ascii_digit() {
            return UNKNOWN;
        }
        version = version.saturating_mul(10).saturating_add(digit - b'0');
    }
    version
}
