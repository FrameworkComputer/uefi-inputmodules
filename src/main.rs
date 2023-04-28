#![no_main]
#![no_std]
#![allow(clippy::result_unit_err)]

extern crate alloc;

use alloc::string::String;

use framework_lib::chromium_ec::{print_err, CrosEc};
#[allow(unused_imports)]
use log::{debug, error, info, trace};
use uefi::prelude::*;
use uefi::proto::shell_params::*;
use uefi::table::boot::{OpenProtocolAttributes, OpenProtocolParams, SearchType};
use uefi::Identify;
#[allow(unused_imports)]
use uefi_services::{print, println};

use getargs::Options;

use crate::commandline::*;
use crate::shell::*;
use crate::uefi_usb::*;

mod commandline;
mod gui;
pub mod hii_image_decode;
pub mod raw_hid;
pub mod shell;
pub mod shell_params;
pub mod uefi_usb;
pub mod via;

const TIMEOUT: u32 = 3000;
const VERBOSE: bool = false;

/// Enable pagination in UEFI shell
///
/// Pagination is handled by the UEFI shell environment automatically, whenever
/// the application prints more than fits on the screen.
fn enable_page_break(boot_services: &BootServices) {
    let shell_handles = boot_services.locate_handle_buffer(SearchType::ByProtocol(&Shell::GUID));
    if let Ok(sh_buf) = shell_handles {
        for handle in &*sh_buf {
            trace!("Calling enable_page_break");
            let shell_handle = unsafe {
                boot_services
                    .open_protocol::<Shell>(
                        OpenProtocolParams {
                            handle: *handle,
                            agent: boot_services.image_handle(),
                            controller: None,
                        },
                        OpenProtocolAttributes::GetProtocol,
                    )
                    .expect("Failed to open Shell handle")
            };
            shell_handle.enable_page_break();
        }
    } else {
        panic!("No shell handle found!");
    }
}

#[entry]
fn main(image_handle: Handle, mut system_table: SystemTable<Boot>) -> Status {
    uefi_services::init(&mut system_table).unwrap();
    let boot_services = system_table.boot_services();

    #[cfg(not(feature = "qemu"))]
    log::set_max_level(log::LevelFilter::Error);

    // TODO: I think I should open this from the ImageHandle?
    let shell_params_h = boot_services
        .locate_handle_buffer(SearchType::ByProtocol(&ShellParameters::GUID))
        .unwrap();
    trace!("Found {} ShellParameters handles", (*shell_params_h).len());
    for handle in &*shell_params_h {
        let params_handle = unsafe {
            boot_services
                .open_protocol::<ShellParameters>(
                    OpenProtocolParams {
                        handle: *handle,
                        agent: boot_services.image_handle(),
                        controller: None,
                    },
                    OpenProtocolAttributes::GetProtocol,
                )
                .expect("Failed to open ShellParameters handle")
        };

        // Ehm why are there two and one has no args?
        // Maybe one is the shell itself?
        if params_handle.argc == 0 {
            continue;
        }

        let args = params_handle.get_args();
        if args.is_empty() {
            gui::draw_background(boot_services, &system_table).unwrap();
        }

        //let args: Vec<_> = std::env::args().skip(1).collect();
        let mut opts = Options::new(args.iter().map(String::as_str));
        let arguments = parse_args(&mut opts);
        match arguments {
            Ok(a) => {
                if a.paginate {
                    enable_page_break(boot_services);
                }
                match a.subcommand {
                    Subcommand::Gui => {
                        info!("gui!");
                        gui::draw_background(boot_services, &system_table).unwrap();

                        //let mut st = unsafe { system_table.unsafe_clone() };
                        //gui::draw_console(&mut st).unwrap();
                    }
                    Subcommand::Lsusb(_) => {
                        get_usb(boot_services, a).unwrap();
                    }
                    Subcommand::Via(_) => {
                        get_usb(boot_services, a).unwrap();
                    }
                    Subcommand::Gop => {
                        gui::print_gop_modes(boot_services).unwrap();
                    }
                    Subcommand::Decoder => {
                        gui::print_image_decoders(boot_services).unwrap();
                    }
                    Subcommand::Tool(args) => {
                        handle_tool(args);
                    }
                }
            }
            Err(_) => {
                println!(
                    r"Usage:
    gui               Show the FL16 Input Module GUI
    lsusb             List usb devices
    via               Simple commandline to control QMK keyboards
    gop               List installed GOP protocols
    decoder           List installed HiiImageDecoder protocols
    tool              Show input module information
    help              Show this help"
                );
                return Status::INVALID_PARAMETER;
            }
        }
    }
    Status::SUCCESS
}

fn handle_tool(args: ToolArgs) {
    let ec = CrosEc::new();

    if args.inputmodules {
        println!("Input Module Status:");
        if let Some(status) = print_err(ec.get_input_deck_status()) {
            println!("Input Deck State: {:?}", status.state);
            println!("Touchpad present: {:?}", status.touchpad_present);
            println!("Positions:");
            println!("  Pos 0: {:?}", status.top_row.pos0);
            println!("  Pos 1: {:?}", status.top_row.pos1);
            println!("  Pos 2: {:?}", status.top_row.pos2);
            println!("  Pos 3: {:?}", status.top_row.pos3);
            println!("  Pos 4: {:?}", status.top_row.pos4);
        } else {
            println!("  Unable to tell");
        }
    }
}
