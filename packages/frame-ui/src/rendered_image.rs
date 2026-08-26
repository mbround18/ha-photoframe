//! The photo currently being shown.
//!
//! Home Assistant prepares every photo to the panel's exact geometry, so this
//! is a plain buffer of pixels in the panel's own format. Stored as RGB565
//! rather than RGBA8: it is what the panel consumes, and at 1280x800 that is
//! 2 MB instead of 4 MB of PSRAM per frame.

use anyhow::{Result, ensure};
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Debug)]
pub struct RenderedImage {
    width: u32,
    height: u32,
    rgb565: Vec<u16>,
}

impl RenderedImage {
    pub fn new(width: u32, height: u32, rgb565: Vec<u16>) -> Result<Self> {
        let expected = (width as usize) * (height as usize);
        ensure!(
            rgb565.len() == expected,
            "pixel buffer is {} pixels, expected {expected} for {width}x{height}",
            rgb565.len(),
        );
        Ok(Self {
            width,
            height,
            rgb565,
        })
    }

    /// Build from 8-bit RGB triplets, which is what an image decoder produces.
    pub fn from_rgb8(width: u32, height: u32, rgb8: &[u8]) -> Result<Self> {
        let expected = (width as usize) * (height as usize) * 3;
        ensure!(
            rgb8.len() == expected,
            "rgb8 buffer is {} bytes, expected {expected} for {width}x{height}",
            rgb8.len(),
        );

        let rgb565 = rgb8
            .chunks_exact(3)
            .map(|px| {
                let r = u16::from(px[0] >> 3) << 11;
                let g = u16::from(px[1] >> 2) << 5;
                let b = u16::from(px[2] >> 3);
                r | g | b
            })
            .collect();

        Self::new(width, height, rgb565)
    }

    /// Build from raw little-endian RGB565, exactly as the panel consumes it.
    ///
    /// This is the normal path: Home Assistant sends pixels already in the
    /// panel's format, so the frame copies them and does no image work at all.
    pub fn from_rgb565_bytes(bytes: &[u8]) -> Result<Self> {
        let width = crate::PANEL_LOGICAL_WIDTH;
        let height = crate::PANEL_LOGICAL_HEIGHT;
        let expected = width * height * 2;
        ensure!(
            bytes.len() == expected,
            "pixel data is {} bytes, expected {expected} for {width}x{height}",
            bytes.len(),
        );

        let rgb565 = bytes
            .chunks_exact(2)
            .map(|px| u16::from_le_bytes([px[0], px[1]]))
            .collect();
        Self::new(width as u32, height as u32, rgb565)
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn rgb565(&self) -> &[u16] {
        &self.rgb565
    }
}

/// How many decoded photos the frame keeps ready.
///
/// One is what is on screen; the rest are the next ones up, already decoded so
/// a transition never waits on a download or a JPEG decode (FR-024). Each costs
/// 2 MB of PSRAM at 1280x800 RGB565, against 32 MB fitted, so three is
/// comfortable. It also means the frame has something to show for a couple of
/// rotations if Home Assistant goes away mid-slideshow.
pub const BUFFER_CAPACITY: usize = 3;

#[derive(Clone, Debug)]
pub struct RenderedImageSnapshot {
    pub image: Option<RenderedImage>,
    /// Bumped on every change so callers can tell "same photo" from "new photo"
    /// without comparing megabytes of pixels.
    pub generation: u64,
    /// How many decoded photos are ready, including the one on screen.
    pub buffered: usize,
}

struct State {
    /// Front is the photo currently on screen; the rest are queued behind it.
    buffer: std::collections::VecDeque<RenderedImage>,
    generation: u64,
}

static STATE: OnceLock<Mutex<State>> = OnceLock::new();

fn state() -> &'static Mutex<State> {
    STATE.get_or_init(|| {
        Mutex::new(State {
            buffer: std::collections::VecDeque::with_capacity(BUFFER_CAPACITY),
            generation: 0,
        })
    })
}

/// Queue a decoded photo behind whatever is on screen.
///
/// The oldest queued photo is dropped once the buffer is full, so a controller
/// that pushes faster than the frame advances cannot grow memory without bound.
pub fn push_rendered_image(image: RenderedImage) -> Result<()> {
    let mut guard = state()
        .lock()
        .map_err(|_| anyhow::anyhow!("rendered image state poisoned"))?;

    if guard.buffer.len() >= BUFFER_CAPACITY {
        // Drop from the back: the front is on screen, and the most recently
        // queued photo is the one most likely to be wanted next.
        guard.buffer.pop_back();
    }
    guard.buffer.push_back(image);

    // Only a change to the visible photo counts as a new generation; queueing
    // behind it must not force a redraw.
    if guard.buffer.len() == 1 {
        guard.generation = guard.generation.wrapping_add(1);
    }
    Ok(())
}

/// Replace whatever is on screen immediately.
pub fn set_rendered_image(image: RenderedImage) -> Result<()> {
    let mut guard = state()
        .lock()
        .map_err(|_| anyhow::anyhow!("rendered image state poisoned"))?;
    guard.buffer.clear();
    guard.buffer.push_back(image);
    guard.generation = guard.generation.wrapping_add(1);
    Ok(())
}

/// Move to the next queued photo, if there is one.
///
/// Returns false when the buffer holds only the current photo, which is the
/// signal that the frame is running dry and needs more from the controller.
pub fn advance_rendered_image() -> Result<bool> {
    let mut guard = state()
        .lock()
        .map_err(|_| anyhow::anyhow!("rendered image state poisoned"))?;

    if guard.buffer.len() < 2 {
        return Ok(false);
    }
    guard.buffer.pop_front();
    guard.generation = guard.generation.wrapping_add(1);
    Ok(true)
}

pub fn clear_rendered_image() -> Result<()> {
    let mut guard = state()
        .lock()
        .map_err(|_| anyhow::anyhow!("rendered image state poisoned"))?;
    guard.buffer.clear();
    guard.generation = guard.generation.wrapping_add(1);
    Ok(())
}

pub fn rendered_image_snapshot() -> Result<RenderedImageSnapshot> {
    let guard = state()
        .lock()
        .map_err(|_| anyhow::anyhow!("rendered image state poisoned"))?;
    Ok(RenderedImageSnapshot {
        image: guard.buffer.front().cloned(),
        generation: guard.generation,
        buffered: guard.buffer.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PIXELS: usize = crate::PANEL_LOGICAL_WIDTH * crate::PANEL_LOGICAL_HEIGHT;

    #[test]
    fn raw_pixels_are_read_little_endian() {
        // Pure red is 0xF800, sent low byte first.
        let bytes: Vec<u8> = std::iter::repeat([0x00, 0xF8])
            .take(PIXELS)
            .flatten()
            .collect();
        let image = RenderedImage::from_rgb565_bytes(&bytes).unwrap();

        assert_eq!(image.width(), crate::PANEL_LOGICAL_WIDTH as u32);
        assert_eq!(image.height(), crate::PANEL_LOGICAL_HEIGHT as u32);
        assert!(image.rgb565().iter().all(|&px| px == 0xF800));
    }

    #[test]
    fn a_truncated_download_is_rejected_rather_than_shown() {
        // Half a photo would otherwise be blitted as garbage.
        let bytes = vec![0u8; PIXELS];
        assert!(RenderedImage::from_rgb565_bytes(&bytes).is_err());
    }

    #[test]
    fn black_survives_the_round_trip() {
        let bytes = vec![0u8; PIXELS * 2];
        let image = RenderedImage::from_rgb565_bytes(&bytes).unwrap();
        assert!(image.rgb565().iter().all(|&px| px == 0));
    }
}
