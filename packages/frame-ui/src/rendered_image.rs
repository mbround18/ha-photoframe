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

#[derive(Clone, Debug)]
pub struct RenderedImageSnapshot {
    pub image: Option<RenderedImage>,
    /// Bumped on every change so callers can tell "same photo" from "new photo"
    /// without comparing megabytes of pixels.
    pub generation: u64,
}

struct State {
    image: Option<RenderedImage>,
    generation: u64,
}

static STATE: OnceLock<Mutex<State>> = OnceLock::new();

fn state() -> &'static Mutex<State> {
    STATE.get_or_init(|| {
        Mutex::new(State {
            image: None,
            generation: 0,
        })
    })
}

pub fn set_rendered_image(image: RenderedImage) -> Result<()> {
    let mut guard = state()
        .lock()
        .map_err(|_| anyhow::anyhow!("rendered image state poisoned"))?;
    guard.image = Some(image);
    guard.generation = guard.generation.wrapping_add(1);
    Ok(())
}

pub fn clear_rendered_image() -> Result<()> {
    let mut guard = state()
        .lock()
        .map_err(|_| anyhow::anyhow!("rendered image state poisoned"))?;
    guard.image = None;
    guard.generation = guard.generation.wrapping_add(1);
    Ok(())
}

pub fn rendered_image_snapshot() -> Result<RenderedImageSnapshot> {
    let guard = state()
        .lock()
        .map_err(|_| anyhow::anyhow!("rendered image state poisoned"))?;
    Ok(RenderedImageSnapshot {
        image: guard.image.clone(),
        generation: guard.generation,
    })
}
