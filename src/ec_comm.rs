//! EC commands that are common to all platforms
use alloc::string::{String, ToString};
use framework_lib::chromium_ec::commands::{
    EcRequestGetHwDiag, EcRequestUefiAppBtnStatus, EcRequestUefiAppMode, EcResponseGetHwDiag,
};
use framework_lib::chromium_ec::{CrosEc, EcError, EcRequestRaw, EcResponseStatus, EcResult};
#[allow(unused_imports)]
use log::{debug, error, info, trace};

pub fn version_info(ec: &Option<CrosEc>) -> String {
    if let Some(ec) = ec {
        if let Ok(version) = ec.version_info() {
            version.to_string()
        } else {
            "???".to_string()
        }
    } else {
        "???".to_string()
    }
}

/// Ask EC if the power button is pressed right now
///
/// If command not supported (early EC version), mock as false.
/// If other error also return false.
/// If no EC (QEMU) also return false.
/// false is a safe default.
pub fn button_pressed(ec: &Option<CrosEc>) -> bool {
    if let Some(ref ec) = ec {
        // TODO: Maybe check some other errors
        match (EcRequestUefiAppBtnStatus {}.send_command(ec)) {
            Ok(btn_status) => btn_status.status == 0,
            // On old EC version
            EcResult::Err(EcError::Response(EcResponseStatus::InvalidCommand)) => false,
            // Other error
            _ => false,
        }
    } else {
        false
    }
}

/// Get Hardware Diagnostics from EC
///
/// If no EC => flag HW diag
/// If any error => no HW diag error
/// This will make sure the system can always boot,
/// but on testing systems (QEMU, 13inch) it will show an issue to help debug.
pub fn get_hw_diag(ec: &Option<CrosEc>) -> EcResponseGetHwDiag {
    let not_ok = EcResponseGetHwDiag {
        // All HW diags flagged (more than possible in reality)
        hw_diag: 0xFFFFFFFF,
        // BIOS not complete, since it's a UEFI app
        bios_complete: 0,
        device_complete: 0,
    };
    let all_ok = EcResponseGetHwDiag {
        // No HW diag issue
        hw_diag: 0,
        // BIOS not complete, since it's a UEFI app
        bios_complete: 0,
        device_complete: 1,
    };

    // Pretend there's no EC because the EC is guaranteed not to have this command
    #[cfg(feature = "azalea")]
    let ec = None;

    let ec = if let Some(ec) = ec {
        ec
    } else {
        // If not EC, fake missing module so we can test the app
        return not_ok;
    };

    match (EcRequestGetHwDiag {}.send_command(ec)) {
        Ok(diag) => diag,
        // Old EC version
        EcResult::Err(EcError::Response(EcResponseStatus::InvalidCommand)) => all_ok,
        // Other error or no EC
        _ => all_ok,
    }
}

/// Set (enable/disable UEFI App mode)
///
/// If error - ignore
/// If no EC - do nothing
/// Safe default.
///
/// The only issue could be if enabling works but disabling fails.
/// Then the system will be in a state where the power button is disabled, which is not good.
/// Only an EC reset will restore it.
pub fn set_uefi_app_mode(ec: &Option<CrosEc>, mode: bool) {
    let enable = if mode { 1 } else { 0 };
    if let Some(ref ec) = ec {
        // Ignore error
        // TODO: Is there anything we should do on error?
        let _res = EcRequestUefiAppMode { enable }.send_command(ec);
    }
}
