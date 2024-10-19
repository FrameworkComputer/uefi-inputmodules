//! Wrapper to take ownership of *Char16

use crate::table::boot::BootServices;
use crate::{CStr16, Char16, Result, Status};
use core::ops::Deref;

/// Wrapper for a string internally allocated from
/// UEFI boot services memory.
#[derive(Debug)]
pub struct PoolString<'a> {
    boot_services: &'a BootServices,
    text: *const Char16,
}

impl<'a> PoolString<'a> {
    /// Create a new poolstring with pointer and boot services to deallocate
    pub fn new(boot_services: &'a BootServices, text: *const Char16) -> Result<Self> {
        if text.is_null() {
            Err(Status::OUT_OF_RESOURCES.into())
        } else {
            Ok(Self {
                boot_services,
                text,
            })
        }
    }
}

impl<'a> Deref for PoolString<'a> {
    type Target = CStr16;

    fn deref(&self) -> &Self::Target {
        unsafe { CStr16::from_ptr(self.text) }
    }
}

impl Drop for PoolString<'_> {
    fn drop(&mut self) {
        let addr = self.text as *mut u8;
        self.boot_services
            .free_pool(addr)
            .expect("Failed to free pool [{addr:#?}]");
    }
}