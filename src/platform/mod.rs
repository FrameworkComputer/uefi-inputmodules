//! Platform abstraction
//!
//! The app can either run as a UEFI application (the default, shipped in the BIOS)
//! or as a native application on a regular OS, for development and debugging.
//! Both backends provide the same set of functions and types.

#[cfg(target_os = "uefi")]
mod efi;
#[cfg(target_os = "uefi")]
pub use efi::*;

#[cfg(not(target_os = "uefi"))]
mod native;
#[cfg(not(target_os = "uefi"))]
pub use native::*;
