use uefi::prelude::*;
use uefi::proto::unsafe_protocol;
use uefi::table::boot::{OpenProtocolAttributes, OpenProtocolParams, ScopedProtocol};

/// EFI_TIMESTAMP_PROPERTIES
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct TimestampProperties {
    /// Counter frequency in Hz
    pub frequency: u64,
    /// Value at which the counter rolls over, e.g. 0xff_ffff for a 24 bit counter
    pub end_value: u64,
}

/// EFI_TIMESTAMP_PROTOCOL
///
/// Optional protocol, added in UEFI 2.4. Our version of uefi-rs doesn't expose
/// it yet, so declare it ourselves.
#[repr(C)]
#[unsafe_protocol("afbfde41-2e6e-4262-ba65-62b9236e5495")]
pub struct Timestamp {
    get_timestamp: unsafe extern "efiapi" fn() -> u64,
    get_properties: unsafe extern "efiapi" fn(*mut TimestampProperties) -> Status,
}

impl Timestamp {
    fn get_timestamp(&self) -> u64 {
        unsafe { (self.get_timestamp)() }
    }

    fn get_properties(&self) -> Option<TimestampProperties> {
        let mut props = TimestampProperties::default();
        if unsafe { (self.get_properties)(&mut props) }.is_success() {
            Some(props)
        } else {
            None
        }
    }
}

/// Measures how long we take, based on EFI_TIMESTAMP_PROTOCOL
///
/// The protocol is optional, so this is None if the firmware doesn't provide it.
/// There is no fallback timer, callers have to handle not knowing the duration.
pub struct Stopwatch<'a> {
    timestamp: ScopedProtocol<'a, Timestamp>,
    properties: TimestampProperties,
    start: u64,
}

impl<'a> Stopwatch<'a> {
    pub fn start(bs: &'a BootServices) -> Option<Self> {
        let properties = get_properties(bs)?;
        // Would divide by zero when converting ticks to microseconds
        if properties.frequency == 0 {
            return None;
        }

        let handle = bs.get_handle_for_protocol::<Timestamp>().ok()?;
        // Read only access to a firmware service, so don't demand exclusive access
        let timestamp = unsafe {
            bs.open_protocol::<Timestamp>(
                OpenProtocolParams {
                    handle,
                    agent: bs.image_handle(),
                    controller: None,
                },
                OpenProtocolAttributes::GetProtocol,
            )
        }
        .ok()?;

        let start = timestamp.get_timestamp();
        Some(Self {
            timestamp,
            properties,
            start,
        })
    }

    pub fn elapsed_us(&self) -> u64 {
        let now = self.timestamp.get_timestamp();
        let ticks = if now >= self.start {
            now - self.start
        } else {
            // The counter rolled over. Can only detect a single rollover, so
            // anything longer than end_value/frequency seconds is wrong.
            self.properties
                .end_value
                .wrapping_sub(self.start)
                .wrapping_add(now)
                .wrapping_add(1)
        };

        // Multiplying first would overflow after a few hours at a 1 GHz counter
        ((ticks as u128 * 1_000_000) / self.properties.frequency as u128) as u64
    }
}

/// Whether the firmware provides EFI_TIMESTAMP_PROTOCOL and how it's configured
pub fn get_properties(bs: &BootServices) -> Option<TimestampProperties> {
    let handle = bs.get_handle_for_protocol::<Timestamp>().ok()?;
    let timestamp = unsafe {
        bs.open_protocol::<Timestamp>(
            OpenProtocolParams {
                handle,
                agent: bs.image_handle(),
                controller: None,
            },
            OpenProtocolAttributes::GetProtocol,
        )
    }
    .ok()?;

    timestamp.get_properties()
}
