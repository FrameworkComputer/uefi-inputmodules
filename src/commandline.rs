use getargs::{Opt, Options};
#[allow(unused_imports)]
use log::{debug, error, info, trace};

#[allow(unused)] // fields are not actually used in this example
#[derive(Debug)]
pub struct Arguments {
    pub verbosity: u32,
    pub paginate: bool,
    pub subcommand: Subcommand,
}

#[derive(Debug)]
pub struct ViaArgs {
    /// Get VIA protocol and config information (most likely NOT what you're looking for)
    pub info: bool,

    ///// Flash device indication (backlight) 3x
    //device_indication: bool,
    /// Set RGB brightness percentage or get, if no value provided
    pub rgb_brightness: Option<Option<u8>>,

    ///// Set RGB effect or get, if no value provided
    //rgb_effect: Option<Option<u8>>,

    ///// Set RGB effect speed or get, if no value provided (0-255)
    //rgb_effect_speed: Option<Option<u8>>,

    ///// Set RGB hue or get, if no value provided. (0-255)
    //rgb_hue: Option<Option<u8>>,

    ///// Set RGB color
    //rgb_color: Option<Color>,

    ///// Set RGB saturation or get, if no value provided. (0-255)
    //rgb_saturation: Option<Option<u8>>,

    ///// Set backlight brightness percentage or get, if no value provided
    //backlight: Option<Option<u8>>,

    ///// Set backlight breathing or get, if no value provided
    //backlight_breathing: Option<Option<bool>>,

    ///// Save RGB/backlight value, otherwise it won't persist through keyboard reboot. Can be used
    ///// by itself or together with other argument.
    //save: bool,

    ///// Reset the EEPROM contents (Not supported by all firmware)
    //eeprom_reset: bool,
    /// Jump to the bootloader (Not supported by all firmware)
    pub bootloader: bool,
}

#[derive(Debug)]
pub struct LsusbArgs {
    /// Filter by vendor ID
    pub vid: Option<u16>,
    /// Filter by product ID
    pub pid: Option<u16>,
}

#[derive(Debug)]
pub struct ToolArgs {
    /// Show input module status
    pub inputmodules: bool,
}

#[allow(unused)] // fields are not actually used in this example
#[derive(Debug)]
pub enum Subcommand {
    Gui,
    Gop,
    Decoder,
    Lsusb(LsusbArgs),
    Via(ViaArgs),
    Tool(ToolArgs),
}

/// Parse all the arguments
pub fn parse_args<'a, I: Iterator<Item = &'a str>>(
    opts: &mut Options<&'a str, I>,
) -> core::result::Result<Arguments, ()> {
    let mut verbosity = 0;
    // TODO: Run
    // gEfiShellProtocol->EnablePageBreak ();
    let mut paginate = false;

    // Parse the top-level options
    while let Some(opt) = opts.next_opt().unwrap() {
        match opt {
            Opt::Short('h') | Opt::Long("help") => return Err(()),
            Opt::Short('b') => paginate = true,
            Opt::Short('v') | Opt::Long("verbose") => verbosity += 1,
            _ => {
                info!("got unknown option {opt:?}");
                return Err(());
            }
        }
    }

    let subcommand_name = opts.next_positional().ok_or(())?;
    let subcommand = match subcommand_name {
        "lsusb" => parse_lsusb_args(opts)?,
        "gui" => parse_gui_args(opts)?,
        "gop" => Subcommand::Gop,
        "decoder" => Subcommand::Decoder,
        "via" => parse_via_args(opts)?,
        "tool" => parse_tool_args(opts)?,
        _ => {
            error!("got unknown subcommand {subcommand_name:?}");
            return Err(());
        }
    };

    Ok(Arguments {
        verbosity,
        subcommand,
        paginate,
    })
}

/// Parse the arguments for the 'via' subcommand
fn parse_via_args<'a, I: Iterator<Item = &'a str>>(
    opts: &mut Options<&'a str, I>,
) -> core::result::Result<Subcommand, ()> {
    // TODO: Use Subcommand::default();
    let mut info = false;
    let mut bootloader = false;
    let mut rgb_brightness = None;
    while let Some(opt) = opts.next_opt().unwrap() {
        match opt {
            Opt::Long("info") => {
                info = true;
            }
            Opt::Long("bootloader") => {
                bootloader = true;
            }
            Opt::Long("rgb-brightness") => {
                rgb_brightness = if let Ok(arg) = opts.value() {
                    Some(arg.parse::<u8>().ok())
                } else {
                    Some(None)
                };
            }
            _ => return Err(()),
        }
    }

    //let args = opts.positionals().collect();
    Ok(Subcommand::Via(ViaArgs {
        info,
        bootloader,
        rgb_brightness,
    }))
}

/// Parse the arguments for the 'lsusb' subcommand
fn parse_gui_args<'a, I: Iterator<Item = &'a str>>(
    _opts: &mut Options<&'a str, I>,
) -> core::result::Result<Subcommand, ()> {
    Ok(Subcommand::Gui {})
}

/// Parse the arguments for the 'lsusb' subcommand
fn parse_lsusb_args<'a, I: Iterator<Item = &'a str>>(
    opts: &mut Options<&'a str, I>,
) -> core::result::Result<Subcommand, ()> {
    let mut pid = None;
    let mut vid = None;
    while let Some(opt) = opts.next_opt().unwrap() {
        match opt {
            Opt::Long("vid") => vid = u16::from_str_radix(opts.value().unwrap(), 16).ok(),
            Opt::Long("pid") => pid = u16::from_str_radix(opts.value().unwrap(), 16).ok(),
            _ => return Err(()),
        }
    }

    Ok(Subcommand::Lsusb(LsusbArgs { vid, pid }))
}

fn parse_tool_args<'a, I: Iterator<Item = &'a str>>(
    opts: &mut Options<&'a str, I>,
) -> core::result::Result<Subcommand, ()> {
    // TODO: Use Subcommand::default();
    let mut inputmodules = false;
    while let Some(opt) = opts.next_opt().unwrap() {
        match opt {
            Opt::Long("inputmodules") => {
                inputmodules = true;
            }
            _ => return Err(()),
        }
    }

    //let args = opts.positionals().collect();
    Ok(Subcommand::Tool(ToolArgs { inputmodules }))
}
