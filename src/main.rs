#![no_main]
#![no_std]
#![allow(clippy::result_unit_err)]

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec::Vec;
#[cfg(not(feature = "qemu"))]
use framework_lib::chromium_ec::CrosEc;
#[allow(unused_imports)]
use log::{debug, error, info, trace};
use uefi::boot::{self, OpenProtocolAttributes, OpenProtocolParams, SearchType};
use uefi::prelude::*;
use uefi::proto::shell_params::ShellParameters;
use uefi::Identify;
#[allow(unused_imports)]
use uefi::{print, println};

use platform::Platform;

mod app;
mod buffer;
mod ec_comm;
mod platform;
mod sleep;
mod uefi_usb;
mod ui;
mod util;
mod built_info {
    // The file has been placed there by the build script.
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

/// Since Rust 1.99 (LLVM 23), loops that search for a NUL u16, like in
/// `CStr16::from_ptr`, are turned into calls to wcslen.
/// There's no libc on UEFI and compiler_builtins doesn't provide it.
///
/// # Safety
/// `s` must point to a NUL-terminated UCS-2 string
#[no_mangle]
pub unsafe extern "C" fn wcslen(s: *const u16) -> usize {
    let mut len = 0;
    // Volatile read, so LLVM can't turn this loop into a call to itself
    while core::ptr::read_volatile(s.add(len)) != 0 {
        len += 1;
    }
    len
}

#[entry]
fn main() -> Status {
    uefi::helpers::init().unwrap();

    // Only error prints in release build
    #[cfg(not(debug_assertions))]
    log::set_max_level(log::LevelFilter::Error);

    let arg = get_shell_action();
    if let Ok(ShellAction::Info) = arg {
        println!("Provide Parameter: 'gui' to launch UI and show issues");
        println!("Provide Parameter: 'force' to launch UI even if everything is ok");
        println!("Provide Parameter: 'gop' to print GOP debugging info");
        println!();
        print_tool_version();
        println!();
        platform::Current::print_status();
    } else if let Ok(ShellAction::GopInfo) = arg {
        println!("GOP Info");
        util::print_gop_info();
    } else {
        info!("No shell arg, launching GUI");

        #[cfg(not(feature = "qemu"))]
        let ec = Some(CrosEc::new());
        #[cfg(feature = "qemu")]
        let ec = None;

        let force_ui = matches!(arg, Ok(ShellAction::Force));

        let show_ui = platform::Current::show_ui();

        let reset = if force_ui || show_ui {
            ec_comm::set_uefi_app_mode(&ec, true);
            let gui_res = app::run::<platform::Current>(&ec, force_ui);
            ec_comm::set_uefi_app_mode(&ec, false);
            gui_res.unwrap()
        } else {
            // If didn't run the app, don't reset, just keep booting
            None
        };

        if let Some(reset_type) = reset {
            #[cfg(debug_assertions)]
            info!("Shutting down");
            boot::stall(core::time::Duration::from_micros(1_000_000));

            // Shut down the system
            // TODO: I should do this outside of this function
            uefi::runtime::reset(reset_type, Status::SUCCESS, None);
        }
    }

    Status::SUCCESS
}

#[derive(Debug)]
enum ShellAction {
    Gui,
    Force,
    Info,
    GopInfo,
}

fn get_shell_action() -> uefi::Result<ShellAction> {
    // TODO: I think I should open this from the ImageHandle?
    let shell_params_h =
        boot::locate_handle_buffer(SearchType::ByProtocol(&ShellParameters::GUID))?;
    trace!("Found {} ShellParameters handles", shell_params_h.len());
    for handle in shell_params_h.iter() {
        let params_handle = unsafe {
            boot::open_protocol::<ShellParameters>(
                OpenProtocolParams {
                    handle: *handle,
                    agent: boot::image_handle(),
                    controller: None,
                },
                OpenProtocolAttributes::GetProtocol,
            )?
        };

        // Ehm why are there two and one has no args?
        // Maybe one is the shell itself?
        if params_handle.args_len() == 0 {
            continue;
        }

        let args: Vec<String> = params_handle.args().map(|arg| arg.to_string()).collect();
        // First arg is the executable path
        return Ok(if args.len() == 2 && args[1] == "gui" {
            ShellAction::Gui
        } else if args.len() == 2 && args[1] == "force" {
            ShellAction::Force
        } else if args.len() == 2 && args[1] == "gop" {
            ShellAction::GopInfo
        } else if args.len() == 1 {
            ShellAction::Gui
        } else {
            ShellAction::Info
        });
    }
    Ok(ShellAction::Gui)
}

fn print_tool_version() {
    let q = "?".to_string();
    println!("Tool Version Information");
    println!("  Version:     {}", built_info::PKG_VERSION);
    #[cfg(debug_assertions)]
    println!("  Built At:    {}", built_info::BUILT_TIME_UTC);
    println!(
        "  Git Commit:  {}",
        built_info::GIT_COMMIT_HASH.unwrap_or(&q)
    );
    println!(
        "  Git Dirty:   {}",
        built_info::GIT_DIRTY
            .map(|x| x.to_string())
            .unwrap_or(q.clone())
    );

    println!(
        "  Built on CI: {:?}",
        built_info::CI_PLATFORM.unwrap_or("None")
    );
    println!(
        "  Git ref:     {:?}",
        built_info::GIT_HEAD_REF.unwrap_or(&q)
    );
    println!("  rustc Ver:   {}", built_info::RUSTC_VERSION);
    println!("  Features     {:?}", built_info::FEATURES);
    println!("  DEBUG:       {}", built_info::DEBUG);
    println!("  Target OS:   {}", built_info::CFG_OS);
}
