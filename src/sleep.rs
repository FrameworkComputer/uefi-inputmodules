use core::ffi::c_void;
use core::ptr::NonNull;
use core::time::Duration;

use uefi::boot::{self, EventType, TimerTrigger, Tpl};
use uefi::Event;

unsafe extern "efiapi" fn idle_callback(_event: Event, _context: Option<NonNull<c_void>>) {}

// Continue execution after a timeout or when the event fires
// This is much better than gBS->Stall because it'll actually pause the CPU instead of busywaiting.
pub fn sleep_or(ms: u64, or_event: Option<Event>) {
    let oneshot_timer = unsafe {
        boot::create_event(EventType::TIMER, Tpl::CALLBACK, Some(idle_callback), None).unwrap()
    };

    boot::set_timer(
        &oneshot_timer,
        TimerTrigger::Relative(Duration::from_millis(ms)),
    )
    .unwrap();

    let mut events = if let Some(event) = or_event {
        [oneshot_timer, event]
    } else {
        [unsafe { oneshot_timer.unsafe_clone() }, oneshot_timer]
    };
    let _which_event_fired = boot::wait_for_event(&mut events).unwrap();
}
