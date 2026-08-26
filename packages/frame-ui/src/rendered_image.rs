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
    /// What the controller calls this photo, when it said.
    ///
    /// Only used to recognise a photo we are already showing. Presenting one
    /// costs a full rotate and a two-megabyte transfer to the panel, which is
    /// visible as a flash -- so doing that to arrive at the picture already on
    /// screen is worse than doing nothing.
    id: Option<String>,
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
            id: None,
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

    /// Tag this photo with the controller's id for it.
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        let id = id.into();
        self.id = (!id.is_empty()).then_some(id);
        self
    }

    pub fn id(&self) -> Option<&str> {
        self.id.as_deref()
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
/// Show this photo now, keeping any spares queued behind it.
///
/// This is what a rotation is: the controller decides it is time for the next
/// picture. Spares already held are kept, so a tap straight afterwards still
/// has something ready.
pub fn show_rendered_image(image: RenderedImage) -> Result<()> {
    let mut guard = state()
        .lock()
        .map_err(|_| anyhow::anyhow!("rendered image state poisoned"))?;

    // Already up. Redrawing it would rotate and blit two megabytes to arrive
    // exactly where we are, which reads as an unexplained flash.
    if let (Some(incoming), Some(showing)) = (
        image.id(),
        guard.buffer.front().and_then(|current| current.id()),
    ) && incoming == showing
    {
        return Ok(());
    }

    if guard.buffer.len() >= BUFFER_CAPACITY {
        // Drop a spare, never the picture being replaced -- that one is about
        // to go anyway.
        guard.buffer.pop_back();
    }
    guard.buffer.push_front(image);
    guard.generation = guard.generation.wrapping_add(1);
    Ok(())
}

/// Hold a photo in reserve, behind whatever is on screen.
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

    /// The photo store is global, so tests that touch it must not overlap.
    /// Held for the body of each such test rather than relying on
    /// `--test-threads=1`, which nothing in the build passes.
    static EXCLUSIVE: Mutex<()> = Mutex::new(());

    fn exclusive() -> std::sync::MutexGuard<'static, ()> {
        // A test that panicked while holding this must not fail every other
        // test after it; the state is reset at the start of each one anyway.
        EXCLUSIVE.lock().unwrap_or_else(|err| err.into_inner())
    }

    fn solid(value: u16) -> RenderedImage {
        RenderedImage::new(
            crate::PANEL_LOGICAL_WIDTH as u32,
            crate::PANEL_LOGICAL_HEIGHT as u32,
            vec![value; PIXELS],
        )
        .unwrap()
    }

    #[test]
    fn showing_the_photo_already_on_screen_does_not_redraw_it() {
        let _guard = exclusive();
        clear_rendered_image().unwrap();
        show_rendered_image(solid(0x1111).with_id("aa11")).unwrap();
        let before = rendered_image_snapshot().unwrap().generation;

        show_rendered_image(solid(0x1111).with_id("aa11")).unwrap();

        // Generation drives the redraw, and a redraw is a full rotate plus a
        // two-megabyte transfer -- visible as a flash for no reason.
        let after = rendered_image_snapshot().unwrap();
        assert_eq!(after.generation, before);
        assert_eq!(after.buffered, 1);
    }

    #[test]
    fn a_different_photo_still_replaces_what_is_on_screen() {
        let _guard = exclusive();
        clear_rendered_image().unwrap();
        show_rendered_image(solid(0x1111).with_id("aa11")).unwrap();
        let before = rendered_image_snapshot().unwrap().generation;

        show_rendered_image(solid(0x2222).with_id("bb22")).unwrap();

        let after = rendered_image_snapshot().unwrap();
        assert_ne!(after.generation, before);
        assert_eq!(after.image.unwrap().rgb565()[0], 0x2222);
    }

    #[test]
    fn an_untagged_photo_is_always_shown() {
        // A controller too old to say which photo this is gets the old
        // behaviour rather than being silently ignored.
        let _guard = exclusive();
        clear_rendered_image().unwrap();
        show_rendered_image(solid(0x1111)).unwrap();
        let before = rendered_image_snapshot().unwrap().generation;

        show_rendered_image(solid(0x1111)).unwrap();

        assert_ne!(rendered_image_snapshot().unwrap().generation, before);
    }

    #[test]
    fn showing_a_photo_replaces_what_is_on_screen_and_keeps_spares() {
        let _guard = exclusive();
        clear_rendered_image().unwrap();
        show_rendered_image(solid(0x1111)).unwrap();
        push_rendered_image(solid(0x2222)).unwrap();
        show_rendered_image(solid(0x3333)).unwrap();

        let snapshot = rendered_image_snapshot().unwrap();
        // The new photo is on screen...
        assert_eq!(snapshot.image.unwrap().rgb565()[0], 0x3333);
        // ...and the spare is still waiting behind it, so a tap has something.
        assert_eq!(snapshot.buffered, 3);
    }

    #[test]
    fn queueing_a_spare_does_not_change_what_is_on_screen() {
        let _guard = exclusive();
        clear_rendered_image().unwrap();
        show_rendered_image(solid(0x1111)).unwrap();
        let before = rendered_image_snapshot().unwrap().generation;

        push_rendered_image(solid(0x2222)).unwrap();

        let after = rendered_image_snapshot().unwrap();
        assert_eq!(after.image.unwrap().rgb565()[0], 0x1111);
        // Generation drives the redraw; queueing must not force one.
        assert_eq!(after.generation, before);
    }

    #[test]
    fn a_tap_moves_to_the_spare() {
        let _guard = exclusive();
        clear_rendered_image().unwrap();
        show_rendered_image(solid(0x1111)).unwrap();
        push_rendered_image(solid(0x2222)).unwrap();

        assert!(advance_rendered_image().unwrap());
        assert_eq!(
            rendered_image_snapshot().unwrap().image.unwrap().rgb565()[0],
            0x2222
        );
    }

    #[test]
    fn a_tap_with_nothing_in_reserve_is_a_harmless_no_op() {
        let _guard = exclusive();
        clear_rendered_image().unwrap();
        show_rendered_image(solid(0x1111)).unwrap();

        assert!(!advance_rendered_image().unwrap());
        assert_eq!(
            rendered_image_snapshot().unwrap().image.unwrap().rgb565()[0],
            0x1111
        );
    }

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
