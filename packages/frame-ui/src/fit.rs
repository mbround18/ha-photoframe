//! Fitting an arbitrary photo to the panel.
//!
//! Home Assistant normally prepares photos to the panel's exact geometry, so
//! this path exists for the one case where it cannot: photos the owner copies
//! straight onto the SD card, which arrive in whatever shape the camera
//! produced.
//!
//! Two treatments, chosen by shape:
//!
//! * **Crop** when the photo is already close to the panel's shape. A 3:2
//!   landscape photo on a 16:10 panel loses a sliver from two edges, which
//!   nobody notices, and fills the screen.
//! * **Contain on black** when it is not. Cropping a portrait photo onto a
//!   landscape panel is the specific failure FR-022 names -- it cuts through
//!   faces. Black bars are honest and, on this panel, invisible: the UI is
//!   already true black because anything lighter shows the panel's mura.
//!
//! Home Assistant's own renderer uses a blurred backdrop rather than black
//! bars. That is a deliberate divergence: a 1280x800 gaussian blur is
//! expensive on a 400 MHz core, and black costs nothing and disappears against
//! the bezel.

use crate::rendered_image::RenderedImage;
use crate::{PANEL_LOGICAL_HEIGHT as HEIGHT, PANEL_LOGICAL_WIDTH as WIDTH};
use anyhow::{Result, ensure};
use image::RgbImage;
use image::imageops::FilterType;

/// How different a photo's shape may be from the panel's before it is
/// letterboxed rather than cropped. Mirrors `_ASPECT_TOLERANCE` in the Home
/// Assistant renderer so both paths make the same call about the same photo.
const ASPECT_TOLERANCE: f32 = 0.25;

/// Resampling filter. Triangle rather than Lanczos3 deliberately: on this CPU
/// the quality difference is invisible at arm's length on a 10" panel, and the
/// cost difference is not.
const FILTER: FilterType = FilterType::Triangle;

/// How a photo was fitted, for logging and for reporting upstream.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Treatment {
    /// Scaled to cover the panel, with the overflow cropped off.
    Crop,
    /// Scaled to fit inside the panel, centred on black.
    ContainOnBlack,
}

/// Scale and pad a decoded photo to exactly the panel's geometry.
pub fn fit_to_panel(source: &RgbImage) -> Result<(RenderedImage, Treatment)> {
    let (src_w, src_h) = source.dimensions();
    ensure!(src_w > 0 && src_h > 0, "image has a zero dimension");

    let target_w = WIDTH as u32;
    let target_h = HEIGHT as u32;

    let src_aspect = src_w as f32 / src_h as f32;
    let target_aspect = target_w as f32 / target_h as f32;
    let treatment = if (src_aspect - target_aspect).abs() / target_aspect <= ASPECT_TOLERANCE {
        Treatment::Crop
    } else {
        Treatment::ContainOnBlack
    };

    // Cover for a crop, contain for a letterbox: the only difference is which
    // way the scale factor is rounded.
    let scale = match treatment {
        Treatment::Crop => (target_w as f32 / src_w as f32).max(target_h as f32 / src_h as f32),
        Treatment::ContainOnBlack => {
            (target_w as f32 / src_w as f32).min(target_h as f32 / src_h as f32)
        }
    };

    // At least one pixel each way, so a wildly elongated photo cannot scale to
    // nothing and panic the resampler.
    let scaled_w = ((src_w as f32 * scale).round() as u32).max(1);
    let scaled_h = ((src_h as f32 * scale).round() as u32).max(1);
    let scaled = image::imageops::resize(source, scaled_w, scaled_h, FILTER);

    // Black, so anything the photo does not cover simply disappears.
    let mut canvas = vec![0u8; (target_w as usize) * (target_h as usize) * 3];

    // Centre the scaled photo. For a crop these offsets are negative, meaning
    // we start reading partway into the source; for a letterbox they are
    // positive, meaning we start writing partway into the canvas. Signed
    // arithmetic handles both without a branch.
    let offset_x = (target_w as i64 - scaled_w as i64) / 2;
    let offset_y = (target_h as i64 - scaled_h as i64) / 2;

    for dst_y in 0..target_h as i64 {
        let src_y = dst_y - offset_y;
        if src_y < 0 || src_y >= scaled_h as i64 {
            continue;
        }
        for dst_x in 0..target_w as i64 {
            let src_x = dst_x - offset_x;
            if src_x < 0 || src_x >= scaled_w as i64 {
                continue;
            }
            let px = scaled.get_pixel(src_x as u32, src_y as u32);
            let base = ((dst_y as usize) * (target_w as usize) + dst_x as usize) * 3;
            canvas[base] = px[0];
            canvas[base + 1] = px[1];
            canvas[base + 2] = px[2];
        }
    }

    let rendered = RenderedImage::from_rgb8(target_w, target_h, &canvas)?;
    Ok((rendered, treatment))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(width: u32, height: u32) -> RgbImage {
        RgbImage::from_pixel(width, height, image::Rgb([255, 0, 0]))
    }

    #[test]
    fn output_is_always_exactly_panel_sized() {
        for (w, h) in [(4000, 3000), (1080, 1920), (100, 100), (5000, 200)] {
            let (fitted, _) = fit_to_panel(&solid(w, h)).unwrap();
            assert_eq!(fitted.width(), WIDTH as u32, "width for {w}x{h}");
            assert_eq!(fitted.height(), HEIGHT as u32, "height for {w}x{h}");
        }
    }

    #[test]
    fn a_photo_shaped_like_the_panel_is_cropped_not_letterboxed() {
        // 16:10, exactly the panel's shape.
        let (_, treatment) = fit_to_panel(&solid(1600, 1000)).unwrap();
        assert_eq!(treatment, Treatment::Crop);
    }

    #[test]
    fn a_portrait_photo_is_letterboxed_rather_than_cropped_through_faces() {
        let (_, treatment) = fit_to_panel(&solid(1080, 1920)).unwrap();
        assert_eq!(treatment, Treatment::ContainOnBlack);
    }

    #[test]
    fn letterbox_bars_are_black_and_the_photo_is_centred() {
        // A tall photo leaves bars down the left and right edges.
        let (fitted, treatment) = fit_to_panel(&solid(1000, 2000)).unwrap();
        assert_eq!(treatment, Treatment::ContainOnBlack);

        let pixels = fitted.rgb565();
        let mid_row = (HEIGHT / 2) * WIDTH;
        assert_eq!(pixels[mid_row], 0, "left edge should be black");
        assert_eq!(pixels[mid_row + WIDTH - 1], 0, "right edge should be black");
        assert_ne!(pixels[mid_row + WIDTH / 2], 0, "centre should be the photo");
    }

    #[test]
    fn a_cropped_photo_leaves_no_black_anywhere() {
        let (fitted, treatment) = fit_to_panel(&solid(1600, 1000)).unwrap();
        assert_eq!(treatment, Treatment::Crop);
        assert!(
            fitted.rgb565().iter().all(|&px| px != 0),
            "a crop should cover the whole panel"
        );
    }

    #[test]
    fn a_zero_sized_image_is_rejected_rather_than_panicking() {
        assert!(fit_to_panel(&RgbImage::new(0, 0)).is_err());
    }
}
