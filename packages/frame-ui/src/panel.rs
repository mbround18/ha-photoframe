//! Panel bring-up and presentation, in Rust against the vendored board BSP.
//!
//! This replaces a hand-written C shim that did the MIPI-DSI and JD9365 setup
//! itself. Doing that by hand meant re-deriving every constant from the
//! schematic and the driver headers, and a wrong one is invisible: the panel
//! reports success and simply never shows anything. That cost a long debugging
//! session where the real fault turned out to be a missing display-on call and
//! a DSI lane configuration that did not match the hardware.
//!
//! Going through the BSP means the lane count, lane bit rate, colour order,
//! panel driver, reset pin and backlight pin are whatever the people who built
//! the board say they are. The BSP is vendored from the manufacturer's own demo
//! bundle, and the bindings come from esp-idf-sys' bindgen via
//! `packages/frame-firmware/bsp_bindings.h`.

#![cfg(target_os = "espidf")]

use anyhow::{Context, Result, anyhow};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::pixelcolor::raw::RawU16;
use embedded_graphics::prelude::*;
use esp_idf_sys as sys;

/// Landscape canvas the UI draws into. The panel is 800x1280 portrait; the
/// frame hangs on its side and the image is rotated on the way out.
pub const WIDTH: usize = crate::PANEL_LOGICAL_WIDTH;
pub const HEIGHT: usize = crate::PANEL_LOGICAL_HEIGHT;

/// Native panel geometry, which is what `esp_lcd_panel_draw_bitmap` wants.
pub const PANEL_WIDTH: usize = 800;
pub const PANEL_HEIGHT: usize = 1280;

pub struct Panel {
    handle: sys::esp_lcd_panel_handle_t,
    /// Landscape drawing surface. Everything draws here, in the orientation a
    /// person sees, and `flush` deals with the panel being physically portrait.
    canvas: Vec<u16>,
    /// The canvas rotated into the panel's own orientation.
    rotated: Vec<u16>,
}

// The handle is an opaque pointer owned solely by this struct.
unsafe impl Send for Panel {}

impl Panel {
    pub fn init() -> Result<Self> {
        let mut config: sys::bsp_display_config_t = unsafe { core::mem::zeroed() };
        // Zero-initialising leaves the lane bit rate at 0, which
        // `esp_lcd_new_dsi_bus` rejects outright rather than substituting the
        // BSP's own default.
        config.dsi_bus.lane_bit_rate_mbps = sys::BSP_LCD_MIPI_DSI_LANE_BITRATE_MBPS;

        let mut handles: sys::bsp_lcd_handles_t = unsafe { core::mem::zeroed() };
        unsafe {
            sys::esp!(sys::bsp_display_new_with_handles(&config, &mut handles))
                .context("bsp_display_new_with_handles failed")?;
        }

        if handles.panel.is_null() {
            return Err(anyhow!("BSP returned a null panel handle"));
        }

        // Without this the panel initialises, answers commands, and scans out
        // nothing -- indistinguishable from a dead panel until you put a camera
        // on it. Some DPI drivers display continuously and report
        // ESP_ERR_NOT_SUPPORTED here, which is not a failure.
        match unsafe { sys::esp_lcd_panel_disp_on_off(handles.panel, true) } {
            sys::ESP_OK => log::info!("panel display on"),
            sys::ESP_ERR_NOT_SUPPORTED => {
                log::info!("panel has no display on/off control; already displaying")
            }
            other => log::warn!("esp_lcd_panel_disp_on_off returned {other}"),
        }

        unsafe {
            sys::esp!(sys::bsp_display_backlight_on()).context("backlight on failed")?;
        }

        log::info!("panel up: {PANEL_WIDTH}x{PANEL_HEIGHT} native, drawing {WIDTH}x{HEIGHT}");

        Ok(Self {
            handle: handles.panel,
            canvas: vec![0u16; WIDTH * HEIGHT],
            rotated: vec![0u16; PANEL_WIDTH * PANEL_HEIGHT],
        })
    }

    /// Set backlight brightness, 0-100 percent.
    pub fn set_backlight(&self, percent: u8) -> Result<()> {
        unsafe {
            sys::esp!(sys::bsp_display_brightness_set(i32::from(percent.min(100))))
                .context("failed to set backlight brightness")?;
        }
        Ok(())
    }

    /// Full-screen solid colours, held long enough to study by eye.
    ///
    /// Solid fills are the right shape for diagnosing ghosting and banding:
    /// there is no image content to alias against, and no rotation or text
    /// rendering involved, so anything visible is the panel or the DSI link
    /// rather than our drawing code.
    ///
    /// Temporary. Remove once the panel is trusted.
    pub fn diagnostic_colour_sweep(&mut self) {
        const DWELL: std::time::Duration = std::time::Duration::from_secs(10);
        for (name, colour) in [
            ("white", 0xFFFFu16),
            ("blue", 0x001F),
            ("red", 0xF800),
            ("green", 0x07E0),
        ] {
            self.rotated.fill(colour);
            match self.blit() {
                Ok(()) => log::info!("diagnostic: {name} on screen for 10s"),
                Err(error) => log::error!("diagnostic: {name} failed: {error:#}"),
            }
            std::thread::sleep(DWELL);
        }
        log::info!("diagnostic: colour sweep complete");
    }

    /// Clear the whole canvas to one colour.
    pub fn clear(&mut self, colour: Rgb565) {
        self.canvas.fill(RawU16::from(colour).into_inner());
    }

    /// Push the canvas to the panel.
    ///
    /// Pixels go through `esp_lcd_panel_draw_bitmap` rather than being written
    /// straight into `esp_lcd_dpi_panel_get_frame_buffer`. Direct writes do not
    /// appear: the CPU's writes to that PSRAM sit in cache while the DMA engine
    /// reads around them. `draw_bitmap` is the cache-safe path.
    pub fn flush(&mut self) -> Result<()> {
        rotate_90(&self.canvas, &mut self.rotated);
        self.blit()
    }

    /// Push `self.rotated` to the panel, cache-flushing it first.
    fn blit(&mut self) -> Result<()> {
        // Flush our writes out of the CPU cache before the DMA engine reads
        // them. The rotated buffer is 2 MB so it lives in PSRAM, and the DPI
        // panel config sets `flags.use_dma2d`. Without this the transfer copies
        // whatever was in PSRAM before the CPU touched it, which presents as a
        // panel that is lit but shows nothing you drew.
        unsafe {
            let bytes = core::mem::size_of_val(self.rotated.as_slice());
            // esp_cache.h: BIT(2) = DIR_C2M (cache to memory), BIT(1) = UNALIGNED.
            // These are BIT() macros, which bindgen does not surface as consts.
            const FLAGS: i32 = (1 << 2) | (1 << 1);
            let err = sys::esp_cache_msync(self.rotated.as_mut_ptr().cast(), bytes, FLAGS);
            if err != sys::ESP_OK {
                log::warn!("esp_cache_msync returned {err}");
            }
        }

        unsafe {
            sys::esp!(sys::esp_lcd_panel_draw_bitmap(
                self.handle,
                0,
                0,
                PANEL_WIDTH as i32,
                PANEL_HEIGHT as i32,
                self.rotated.as_ptr().cast(),
            ))
            .context("esp_lcd_panel_draw_bitmap failed")?;
        }
        Ok(())
    }

    /// Blit an already-prepared landscape RGB565 image straight to the panel.
    /// Used for photos, which arrive at exactly the panel's geometry and need
    /// no drawing on top.
    pub fn present(&mut self, pixels: &[u16]) -> Result<()> {
        if pixels.len() < WIDTH * HEIGHT {
            return Err(anyhow!(
                "frame too small: {} pixels, need {}",
                pixels.len(),
                WIDTH * HEIGHT
            ));
        }
        self.canvas[..WIDTH * HEIGHT].copy_from_slice(&pixels[..WIDTH * HEIGHT]);
        self.flush()
    }
}

impl OriginDimensions for Panel {
    fn size(&self) -> Size {
        Size::new(WIDTH as u32, HEIGHT as u32)
    }
}

impl DrawTarget for Panel {
    type Color = Rgb565;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> core::result::Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(point, colour) in pixels {
            if point.x < 0 || point.y < 0 {
                continue;
            }
            let (x, y) = (point.x as usize, point.y as usize);
            if x >= WIDTH || y >= HEIGHT {
                continue;
            }
            self.canvas[y * WIDTH + x] = RawU16::from(colour).into_inner();
        }
        Ok(())
    }
}

/// Rotate a `WIDTH x HEIGHT` landscape frame 90 degrees into a
/// `PANEL_WIDTH x PANEL_HEIGHT` portrait buffer.
///
/// This is the CPU-side rotation the panel cannot do itself (it rejects
/// `esp_lcd_panel_swap_xy`). Task T034 moves it onto the PPA.
///
/// The direction is set by how the frame physically hangs: 270 degrees puts the
/// image upside down on this board, confirmed by reading text off the panel.
fn rotate_90(src: &[u16], dst: &mut [u16]) {
    debug_assert_eq!(dst.len(), PANEL_WIDTH * PANEL_HEIGHT);
    for y in 0..HEIGHT {
        let row = &src[y * WIDTH..y * WIDTH + WIDTH];
        for (x, pixel) in row.iter().enumerate() {
            // 90 degrees: (x, y) -> (HEIGHT - 1 - y, x)
            dst[x * PANEL_WIDTH + (HEIGHT - 1 - y)] = *pixel;
        }
    }
}
