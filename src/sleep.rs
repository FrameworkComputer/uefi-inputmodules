use core::ffi::c_void;
use core::ptr::NonNull;

#[allow(unused_imports)]
use uefi::prelude::*;
use uefi::table::boot::{BootServices, EventType, Tpl};
use uefi::Event;

unsafe extern "efiapi" fn idle_callback(_event: Event, _context: Option<NonNull<c_void>>) {}

// Continue execution after a timeout or when the event fires
// This is much better than gBS->Stall because it'll actually pause the CPU instead of busywaiting.
pub fn sleep_or(bs: &BootServices, ms: u64, or_event: Option<Event>) {
    let oneshot_timer = unsafe {
        bs.create_event(EventType::TIMER, Tpl::CALLBACK, Some(idle_callback), None)
            .unwrap()
    };

    bs.set_timer(
        &oneshot_timer,
        uefi::table::boot::TimerTrigger::Relative(ms * 10_000),
    )
    .unwrap();

    let mut events = if let Some(event) = or_event {
        [oneshot_timer, event]
    } else {
        [unsafe { oneshot_timer.unsafe_clone() }, oneshot_timer]
    };
    let _which_event_fired = bs.wait_for_event(&mut events).discard_errdata().unwrap();
}
