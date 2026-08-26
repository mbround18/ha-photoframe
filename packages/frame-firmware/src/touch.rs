//! Tap the picture to see the next one.
//!
//! The only interaction an adopted frame has. There is no on-screen control to
//! find, nothing to learn, and nothing drawn to say it exists -- an adopted
//! frame shows photos and nothing else (Principle VIII). Someone who never
//! discovers it loses nothing; someone who taps gets the next photo.
//!
//! Fails soft on purpose. The GSL3680 driver this needs is vendored with a
//! weaker licence position than anything else in the tree (see its
//! PROVENANCE.md), so it must be removable: if touch cannot start, the frame
//! logs it once and carries on exactly as it did before touch existed.

#![cfg(target_os = "espidf")]

use esp_idf_svc::sys;
use std::time::{Duration, Instant};

/// How long after a tap before another is accepted.
///
/// Long enough to swallow the second contact of a double-tap and the wobble of
/// a finger resting on glass, short enough that someone flicking through
/// photos never feels blocked.
const TAP_COOLDOWN: Duration = Duration::from_millis(600);

/// How often the controller is polled.
///
/// The interrupt line is wired, but polling is simpler and 50 ms is well below
/// what anyone perceives as lag on a deliberate tap.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Start watching for taps. Returns false if touch is unavailable.
///
/// `on_tap` runs on the touch thread and should be quick; advancing the photo
/// is a queue push, not a render.
pub fn start<F>(on_tap: F) -> bool
where
    F: Fn() + Send + 'static,
{
    let mut handle: sys::esp_lcd_touch_handle_t = std::ptr::null_mut();
    let result = unsafe { sys::bsp_touch_new(std::ptr::null(), &mut handle) };

    if result != sys::ESP_OK || handle.is_null() {
        log::warn!(
            "touch unavailable (esp_err {result}); the frame will run without \
             tap-to-advance"
        );
        return false;
    }

    // Small stack: this thread reads a few coordinates and calls a closure that
    // pushes to a queue. It must still be set explicitly -- ESP-IDF's 3 KB
    // pthread default is not a size anything here should inherit by accident.
    let handle = TouchHandle(handle);
    crate::runtime::spawn_with_psram_stack(c"frame-touch", 8 * 1024, move || run(handle, on_tap));
    log::info!("touch ready: tap the screen for the next photo");
    true
}

/// The touch handle, moved to the polling thread.
///
/// ESP-IDF's handle is a raw pointer and so not `Send` by default. It is safe
/// to move here because exactly one thread ever touches it: this one owns it
/// for the life of the program and nothing else keeps a copy.
struct TouchHandle(sys::esp_lcd_touch_handle_t);
unsafe impl Send for TouchHandle {}

fn run<F: Fn()>(handle: TouchHandle, on_tap: F) {
    let handle = handle.0;
    // Tracks the finger going down rather than being down, so resting a hand on
    // the glass advances one photo instead of racing through the whole album.
    let mut was_touching = false;
    let mut last_tap: Option<Instant> = None;

    loop {
        std::thread::sleep(POLL_INTERVAL);

        if unsafe { sys::esp_lcd_touch_read_data(handle) } != sys::ESP_OK {
            // A bad read is not worth a log line every 50 ms; the next one
            // usually succeeds, and a controller that has truly gone away
            // simply means no more taps.
            continue;
        }

        let mut xs = [0u16; 1];
        let mut ys = [0u16; 1];
        let mut strengths = [0u16; 1];
        let mut count: u8 = 0;
        let touching = unsafe {
            sys::esp_lcd_touch_get_coordinates(
                handle,
                xs.as_mut_ptr(),
                ys.as_mut_ptr(),
                strengths.as_mut_ptr(),
                &mut count,
                1,
            )
        } && count > 0;

        let pressed = touching && !was_touching;
        was_touching = touching;

        if !pressed {
            continue;
        }

        let now = Instant::now();
        if last_tap.is_some_and(|previous| now.duration_since(previous) < TAP_COOLDOWN) {
            continue;
        }
        last_tap = Some(now);

        log::debug!("tap at {},{}", xs[0], ys[0]);
        on_tap();
    }
}
