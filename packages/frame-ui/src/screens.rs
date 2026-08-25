//! The frame's on-device screens.
//!
//! There are only three, and two of them exist purely to get the frame adopted.
//! Once Home Assistant is driving it the panel shows photos and nothing else --
//! no status text, no addresses, no error codes (Constitution Principle VIII).
//!
//! Drawn with embedded-graphics directly rather than a UI toolkit. The frame
//! needs a handful of centred strings on a flat background; a retained-mode
//! toolkit costs more in flash than the entire rest of the firmware and buys
//! nothing here.

#![cfg(target_os = "espidf")]

use anyhow::Result;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::mono_font::ascii::{FONT_6X13, FONT_9X18_BOLD, FONT_10X20};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle, RoundedRectangle};
use embedded_graphics::text::{Alignment, Text};

use crate::panel::{HEIGHT, Panel, WIDTH};

/// Deep navy. Dark enough to be restful in a living room at night, which is
/// where this thing lives.
const BACKGROUND: Rgb565 = Rgb565::new(1, 3, 6);
const FOREGROUND: Rgb565 = Rgb565::WHITE;
const MUTED: Rgb565 = Rgb565::new(18, 37, 22);
const ACCENT: Rgb565 = Rgb565::new(7, 47, 31);
const PANEL_FILL: Rgb565 = Rgb565::new(2, 6, 11);

const CENTRE_X: i32 = (WIDTH / 2) as i32;

fn heading(text: &str, y: i32, target: &mut Panel) {
    Text::with_alignment(
        text,
        Point::new(CENTRE_X, y),
        MonoTextStyle::new(&FONT_10X20, FOREGROUND),
        Alignment::Center,
    )
    .draw(target)
    .ok();
}

fn body(text: &str, y: i32, target: &mut Panel) {
    Text::with_alignment(
        text,
        Point::new(CENTRE_X, y),
        MonoTextStyle::new(&FONT_6X13, MUTED),
        Alignment::Center,
    )
    .draw(target)
    .ok();
}

/// A boxed, high-contrast value: the thing the person has to read and type.
fn callout(text: &str, y: i32, target: &mut Panel) {
    let box_w = 620i32;
    let box_h = 92i32;
    let top_left = Point::new(CENTRE_X - box_w / 2, y);

    RoundedRectangle::with_equal_corners(
        Rectangle::new(top_left, Size::new(box_w as u32, box_h as u32)),
        Size::new(16, 16),
    )
    .into_styled(PrimitiveStyle::with_fill(PANEL_FILL))
    .draw(target)
    .ok();

    Text::with_alignment(
        text,
        Point::new(CENTRE_X, y + box_h / 2 + 8),
        MonoTextStyle::new(&FONT_9X18_BOLD, ACCENT),
        Alignment::Center,
    )
    .draw(target)
    .ok();
}

/// Shown while the frame is still bringing itself up.
pub fn show_starting(panel: &mut Panel) -> Result<()> {
    panel.clear(BACKGROUND);
    heading("Starting up", (HEIGHT / 2) as i32 - 10, panel);
    body("Just a moment.", (HEIGHT / 2) as i32 + 24, panel);
    panel.flush()
}

/// Shown while the frame is waiting to be put on Wi-Fi.
pub fn show_wifi_setup(panel: &mut Panel, ssid: &str) -> Result<()> {
    panel.clear(BACKGROUND);
    heading("Connect this frame to Wi-Fi", 190, panel);
    body(
        "Join the network below from your phone, then follow the prompt.",
        232,
        panel,
    );
    callout(ssid, 300, panel);
    panel.flush()
}

/// Shown once the frame is online but no Home Assistant has claimed it.
///
/// The frame ID is on screen because otherwise the only way to find it is a
/// serial log, and nobody setting up a gift should need a serial cable. It
/// disappears the moment the frame is adopted.
pub fn show_awaiting_adoption(panel: &mut Panel, frame_id: &str) -> Result<()> {
    panel.clear(BACKGROUND);
    heading("Add this frame in Home Assistant", 170, panel);
    body(
        "Settings > Devices & Services > Add Integration > PhotoFrame Bridge",
        212,
        panel,
    );
    body("then enter the ID below.", 234, panel);
    callout(frame_id, 296, panel);
    body("Photos will start appearing here on their own.", 430, panel);
    panel.flush()
}

/// Shown after adoption, before the first photo arrives.
///
/// Deliberately almost empty: this is the last thing a person sees before the
/// frame becomes a picture frame, and it should already feel like one.
pub fn show_ready(panel: &mut Panel) -> Result<()> {
    panel.clear(BACKGROUND);
    body("Waiting for photos", (HEIGHT / 2) as i32, panel);
    panel.flush()
}
