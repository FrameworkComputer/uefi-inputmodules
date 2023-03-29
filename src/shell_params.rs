use alloc::borrow::ToOwned;
use core::ffi::c_void;
use uefi::proto::unsafe_protocol;
use uefi::{CStr16, CString16, Char16};

use alloc::string::String;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;

type ShellFileHandle = *const c_void;

#[repr(C)]
#[unsafe_protocol("752f3136-4e16-4fdc-a22a-e5f46812f4ca")]
pub struct ShellParameters {
    pub argv: *const *const Char16,
    pub argc: usize,
    pub std_in: ShellFileHandle,
    pub std_out: ShellFileHandle,
    pub std_err: ShellFileHandle,
}
impl ShellParameters {
    pub fn get_args(&self) -> Vec<CString16> {
        let mut args = vec![];
        // Skip first one, which is the executable's name
        for i in 1..self.argc {
            let str = unsafe { CStr16::from_ptr(*self.argv.add(i)) };
            let string = str.to_owned();
            args.push(string);
        }
        args
    }
    pub fn get_str_args(&self) -> Vec<String> {
        let mut args = vec![];
        // Skip first one, which is the executable's name
        for i in 1..self.argc {
            let str = unsafe { CStr16::from_ptr(*self.argv.add(i)) };
            let string = str.to_string();
            args.push(string);
        }
        args
    }
}
