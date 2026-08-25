use anyhow::{Context, Result, ensure};
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderedImage {
    pub width: u32,
    pub height: u32,
    pub rgba8: Vec<u8>,
}

impl RenderedImage {
    pub fn new(width: u32, height: u32, rgba8: Vec<u8>) -> Result<Self> {
        let expected_len = width as usize * height as usize * 4;
        ensure!(
            rgba8.len() == expected_len,
            "decoded image buffer length {} did not match expected RGBA size {}",
            rgba8.len(),
            expected_len
        );

        Ok(Self {
            width,
            height,
            rgba8,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderedImageSnapshot {
    pub generation: u64,
    pub image: Option<RenderedImage>,
}

#[derive(Default)]
struct RenderedImageState {
    generation: u64,
    image: Option<RenderedImage>,
}

fn rendered_image_state() -> &'static Mutex<RenderedImageState> {
    static RENDERED_IMAGE_STATE: OnceLock<Mutex<RenderedImageState>> = OnceLock::new();
    RENDERED_IMAGE_STATE.get_or_init(|| Mutex::new(RenderedImageState::default()))
}

pub fn set_rendered_image(image: RenderedImage) -> Result<()> {
    let mut state = rendered_image_state()
        .lock()
        .map_err(|_| anyhow::anyhow!("rendered image state lock poisoned"))?;
    state.generation = state.generation.wrapping_add(1);
    state.image = Some(image);
    Ok(())
}

pub fn clear_rendered_image() -> Result<()> {
    let mut state = rendered_image_state()
        .lock()
        .map_err(|_| anyhow::anyhow!("rendered image state lock poisoned"))?;
    state.generation = state.generation.wrapping_add(1);
    state.image = None;
    Ok(())
}

pub fn rendered_image_snapshot() -> Result<RenderedImageSnapshot> {
    let state = rendered_image_state()
        .lock()
        .map_err(|_| anyhow::anyhow!("rendered image state lock poisoned"))
        .context("failed to read rendered image state")?;
    Ok(RenderedImageSnapshot {
        generation: state.generation,
        image: state.image.clone(),
    })
}
