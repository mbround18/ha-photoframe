//! The frame's on-device screens.
//!
//! There are only three, and two exist purely to get the frame adopted. Once
//! Home Assistant is driving it the panel shows photos and nothing else -- no
//! status text, no addresses, no error codes (Constitution Principle VIII).
//!
//! Everything is drawn on **true black** and nothing else is filled. That is a
//! hardware decision, not a taste one: this panel shows visible vertical
//! banding and ghosting in near-black levels, while pure black drives the
//! pixels fully closed and hides it completely. Solid full-screen colours were
//! confirmed clean on this panel, so any large dark fill we draw ourselves is
//! the thing that shows mura. It also happens to be the right look for a photo
//! frame in a dark room.
//!
//! Drawn with embedded-graphics directly. The frame needs a handful of centred
//! strings; a retained-mode toolkit costs more in flash than the rest of the
//! firmware and buys nothing here.

#![cfg(target_os = "espidf")]

use anyhow::Result;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::mono_font::ascii::{FONT_9X15, FONT_10X20};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::text::{Alignment, Text};

use crate::panel::{HEIGHT, Panel, WIDTH};

/// True black. Anything above this shows the panel's banding.
const BACKGROUND: Rgb565 = Rgb565::BLACK;
const PRIMARY: Rgb565 = Rgb565::WHITE;
/// Dim grey for supporting lines. Kept well clear of the near-black range.
const SECONDARY: Rgb565 = Rgb565::new(18, 37, 18);

const CENTRE_X: i32 = (WIDTH / 2) as i32;

fn line(text: &str, y: i32, colour: Rgb565, large: bool, target: &mut Panel) {
    let style = if large {
        MonoTextStyle::new(&FONT_10X20, colour)
    } else {
        MonoTextStyle::new(&FONT_9X15, colour)
    };
    Text::with_alignment(text, Point::new(CENTRE_X, y), style, Alignment::Center)
        .draw(target)
        .ok();
}

/// Shown while the frame is still bringing itself up.
pub fn show_starting(panel: &mut Panel) -> Result<()> {
    panel.clear(BACKGROUND);
    line("Starting up", (HEIGHT / 2) as i32, PRIMARY, true, panel);
    panel.flush()
}

/// Shown while the frame is waiting to be put on Wi-Fi.
pub fn show_wifi_setup(panel: &mut Panel, ssid: &str) -> Result<()> {
    panel.clear(BACKGROUND);
    line("Connect this frame to Wi-Fi", 300, PRIMARY, true, panel);
    line(
        "Join this network from your phone, then follow the prompt.",
        350,
        SECONDARY,
        false,
        panel,
    );
    line(ssid, 430, PRIMARY, true, panel);
    panel.flush()
}

/// Shown once the frame is online but no Home Assistant has claimed it.
///
/// The frame ID is here because the only other place it appears is a serial
/// log, and nobody setting up a gift should need a serial cable. It disappears
/// the moment the frame is adopted.
pub fn show_awaiting_adoption(panel: &mut Panel, frame_id: &str) -> Result<()> {
    panel.clear(BACKGROUND);
    line(
        "Add this frame in Home Assistant",
        270,
        PRIMARY,
        true,
        panel,
    );
    line(
        "Settings > Devices & Services > Add Integration > PhotoFrame Bridge",
        318,
        SECONDARY,
        false,
        panel,
    );
    line(frame_id, 400, PRIMARY, true, panel);
    line(
        "Photos will start appearing here on their own.",
        470,
        SECONDARY,
        false,
        panel,
    );
    panel.flush()
}

/// Shown after adoption, before the first photo arrives.
///
/// Deliberately almost nothing: this is the last thing seen before the frame
/// becomes a picture frame, and it should already feel like one.
pub fn show_ready(panel: &mut Panel) -> Result<()> {
    panel.clear(BACKGROUND);
    line(
        "Waiting for photos",
        (HEIGHT / 2) as i32,
        SECONDARY,
        false,
        panel,
    );
    panel.flush()
}
