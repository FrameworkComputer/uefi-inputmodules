//! Native backend: Draw into a desktop window and read keys from it.
//!
//! Everything that would affect the running system (resetting, shutting down,
//! writing UEFI variables, putting the EC into UEFI app mode) only logs what
//! it would have done.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use framework_lib::chromium_ec::CrosEc;
#[allow(unused_imports)]
use log::{debug, error, info, trace, warn};
use minifb::{InputCallback, KeyRepeat, Scale, ScaleMode, Window, WindowOptions};
use uefi::proto::console::text::{Key, ScanCode};
use uefi::runtime::ResetType;
use uefi::{Char16, Status};

use crate::buffer::Buffer;
use crate::drawing::{HEIGHT, WIDTH};
use crate::BiosSettings;

mod usb;

pub use usb::get_usb_devices;

const WINDOW_TITLE: &str = "Framework Laptop 16 - Inputmodules";

/// Collects typed characters, respecting the keyboard layout and modifiers
struct CharQueue(Rc<RefCell<VecDeque<char>>>);

impl InputCallback for CharQueue {
    fn add_char(&mut self, uni_char: u32) {
        if let Some(c) = char::from_u32(uni_char).filter(|c| !c.is_control()) {
            self.0.borrow_mut().push_back(c);
        }
    }
}

/// Desktop window, emulating the GOP display and console input
pub struct Screen {
    window: Window,
    /// Window framebuffer in 0RGB format
    framebuffer: Vec<u32>,
    keys: VecDeque<Key>,
    chars: Rc<RefCell<VecDeque<char>>>,
}

static WINDOWED: AtomicBool = AtomicBool::new(false);

/// Show the UI in a regular resizable window instead of fullscreen
pub fn set_windowed(windowed: bool) {
    WINDOWED.store(windowed, Ordering::Relaxed);
}

/// Size of the primary screen in physical pixels
#[cfg(windows)]
fn screen_size() -> Option<(usize, usize)> {
    use windows_sys::Win32::UI::HiDpi::{
        SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};
    let (width, height) = unsafe {
        // Without DPI awareness Windows reports a scaled-down screen size
        // and stretches the window, which makes the UI blurry.
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN))
    };
    (width > 0 && height > 0).then_some((width as usize, height as usize))
}

/// Size of the default X11 screen in pixels
#[cfg(all(unix, not(target_os = "macos")))]
fn screen_size() -> Option<(usize, usize)> {
    let xlib = x11_dl::xlib::Xlib::open()
        .map_err(|err| error!("Failed to load Xlib: {}", err))
        .ok()?;
    unsafe {
        let display = (xlib.XOpenDisplay)(std::ptr::null());
        if display.is_null() {
            error!("Failed to open X display");
            return None;
        }
        let screen = (xlib.XDefaultScreen)(display);
        let width = (xlib.XDisplayWidth)(display, screen);
        let height = (xlib.XDisplayHeight)(display, screen);
        (xlib.XCloseDisplay)(display);
        (width > 0 && height > 0).then_some((width as usize, height as usize))
    }
}

#[cfg(not(any(windows, all(unix, not(target_os = "macos")))))]
fn screen_size() -> Option<(usize, usize)> {
    None
}

impl Screen {
    pub fn open() -> Option<Self> {
        // The UI is laid out for the 2560x1600 display of the Framework Laptop 16.
        // Keep the buffer at that size and let the window scale it to fit.
        let fullscreen = if WINDOWED.load(Ordering::Relaxed) {
            None
        } else {
            let size = screen_size();
            if size.is_none() {
                warn!("Couldn't determine screen size, falling back to windowed mode");
            }
            size
        };

        let mut window = if let Some((width, height)) = fullscreen {
            debug!("Opening fullscreen window: {}x{}", width, height);
            // Borderless window covering the entire screen
            let mut window = Window::new(
                WINDOW_TITLE,
                width,
                height,
                WindowOptions {
                    none: true,
                    topmost: true,
                    scale: Scale::X1,
                    scale_mode: ScaleMode::AspectRatioStretch,
                    ..WindowOptions::default()
                },
            )
            .map_err(|err| error!("Failed to open window: {}", err))
            .ok()?;
            window.set_position(0, 0);
            window
        } else {
            Window::new(
                WINDOW_TITLE,
                WIDTH / 2,
                HEIGHT / 2,
                WindowOptions {
                    resize: true,
                    scale: Scale::X1,
                    scale_mode: ScaleMode::AspectRatioStretch,
                    ..WindowOptions::default()
                },
            )
            .map_err(|err| error!("Failed to open window: {}", err))
            .ok()?
        };

        let chars = Rc::new(RefCell::new(VecDeque::new()));
        window.set_input_callback(Box::new(CharQueue(chars.clone())));

        Some(Screen {
            window,
            framebuffer: vec![0; WIDTH * HEIGHT],
            keys: VecDeque::new(),
            chars,
        })
    }

    pub fn size(&self) -> (usize, usize) {
        (WIDTH, HEIGHT)
    }

    pub fn blit(&mut self, buffer: &Buffer) -> uefi::Result {
        let (width, height) = (buffer.width(), buffer.height());
        self.framebuffer.resize(width * height, 0);
        for (dst, src) in self.framebuffer.iter_mut().zip(buffer.pixels()) {
            *dst = ((src.red as u32) << 16) | ((src.green as u32) << 8) | (src.blue as u32);
        }
        self.window
            .update_with_buffer(&self.framebuffer, width, height)
            .map_err(|err| {
                error!("Failed to draw to window: {}", err);
                uefi::Error::from(Status::DEVICE_ERROR)
            })?;
        self.collect_keys();
        Ok(())
    }

    /// Process window events and queue newly pressed keys
    fn collect_keys(&mut self) {
        // Special keys that don't produce a character
        for key in self.window.get_keys_pressed(KeyRepeat::No) {
            trace!("Key pressed: {:?}", key);
            let special = match key {
                minifb::Key::Escape => ScanCode::ESCAPE,
                minifb::Key::F12 => ScanCode::FUNCTION_12,
                _ => continue,
            };
            self.keys.push_back(Key::Special(special));
        }
        // Printable characters
        for c in self.chars.borrow_mut().drain(..) {
            trace!("Char typed: {:?}", c);
            if let Ok(c) = Char16::try_from(c) {
                self.keys.push_back(Key::Printable(c));
            }
        }
        // Closing the window is the same as pressing ESC to exit
        if !self.window.is_open() {
            self.keys.push_back(Key::Special(ScanCode::ESCAPE));
        }
    }

    /// Sleep until a key is pressed or the timeout expires, without consuming the key
    pub fn wait(&mut self, ms: u64) {
        let deadline = Instant::now() + Duration::from_millis(ms);
        loop {
            self.window.update();
            self.collect_keys();
            let now = Instant::now();
            if !self.keys.is_empty() || now >= deadline {
                break;
            }
            std::thread::sleep((deadline - now).min(Duration::from_millis(10)));
        }
    }

    /// Sleep until a key is pressed or the timeout expires, then return the pressed key, if any
    pub fn wait_for_key(&mut self, ms: u64) -> Option<Key> {
        self.wait(ms);
        self.keys.pop_front()
    }
}

pub fn print_gop_info() -> Option<()> {
    println!(
        "No GOP when running natively. Window buffer: {}x{}",
        WIDTH, HEIGHT
    );
    Some(())
}

pub fn stall(duration: Duration) {
    std::thread::sleep(duration);
}

/// Seconds since the start of the current hour
pub fn clock_seconds() -> Option<u32> {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
    Some((secs % 3600) as u32)
}

pub fn reset(reset_type: ResetType) {
    info!(
        "Would do {:?} reset now. Not doing it natively.",
        reset_type
    );
}

pub fn force_cold_reset() {
    info!("Would force cold reset via CF9 now. Not doing it natively.");
}

pub fn set_uefi_app_mode(_ec: &Option<CrosEc>, mode: bool) {
    debug!("Not setting EC UEFI app mode to {} natively", mode);
}

pub fn set_os_indications_setupmenu() {
    info!("Would set OsIndications to boot into BIOS setup. Not doing it natively.");
}

/// The OEM variable protocol is only available in UEFI
pub fn get_bios_settings() -> Option<BiosSettings> {
    None
}
