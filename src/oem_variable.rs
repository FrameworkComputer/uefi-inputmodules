use uefi::proto::unsafe_protocol;

#[derive(PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum InputmoduleSetting {
    ForceOff,
    RequireModules,
    ForceOn,
}

/// EFI_OEM_VARIABLE_AREA
#[repr(C, packed)]
#[derive(Debug)]
pub struct OemVariableArea {
    /// 0: Disable, 1: Enable
    standalone_mode: u8,
    /// 0: Force Off, 1:Require, 2: Force On
    inputmodule_request: u8,
}

/// EFI_OEM_VARIABLE_AREA_PROTOCOL
#[repr(C)]
#[unsafe_protocol("59aac985-cca5-4270-a44e-f1930916d6f0")]
pub struct UefiVariable {
    pub oem_variable_area_ptr: *const OemVariableArea,
}

impl UefiVariable {
    pub fn standalone_mode_enabled(&self) -> bool {
        unsafe { (*self.oem_variable_area_ptr).standalone_mode == 1 }
    }
    /// Raw value, so we can save exactly what the BIOS gave us
    pub fn standalone_mode_raw(&self) -> u8 {
        unsafe { (*self.oem_variable_area_ptr).standalone_mode }
    }
    /// Raw value, so we can save exactly what the BIOS gave us, even if invalid
    pub fn inputmodule_setting_raw(&self) -> u8 {
        unsafe { (*self.oem_variable_area_ptr).inputmodule_request }
    }
    pub fn inputmodule_setting(&self) -> Option<InputmoduleSetting> {
        let setting = unsafe { (*self.oem_variable_area_ptr).inputmodule_request };
        match setting {
            0 => Some(InputmoduleSetting::ForceOff),
            1 => Some(InputmoduleSetting::RequireModules),
            2 => Some(InputmoduleSetting::ForceOn),
            _ => None,
        }
    }
}
