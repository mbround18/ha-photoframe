use anyhow::Result;

use crate::display::EmbeddedDisplayConfig;

#[cfg(target_os = "espidf")]
use slint::platform::software_renderer::MinimalSoftwareWindow;

#[cfg(target_os = "espidf")]
use core::ffi::c_void;

#[cfg(target_os = "espidf")]
use std::cell::Cell;

#[cfg(target_os = "espidf")]
use anyhow::anyhow;

#[cfg(target_os = "espidf")]
use slint::{
    LogicalPosition,
    platform::{PointerEventButton, WindowAdapter, WindowEvent},
};

#[cfg(target_os = "espidf")]
unsafe extern "C" {
    fn bsp_touch_new(config: *const c_void, ret_touch: *mut *mut c_void) -> i32;
    fn esp_lcd_touch_read_data(handle: *mut c_void) -> i32;
    fn esp_lcd_touch_get_coordinates(
        handle: *mut c_void,
        x: *mut u16,
        y: *mut u16,
        strength: *mut u16,
        point_count: *mut u8,
        max_point_num: u8,
    ) -> bool;
}

#[cfg(target_os = "espidf")]
const EAGER_TOUCH_BRINGUP_ENABLED: bool = false;

#[derive(Debug)]
pub struct EmbeddedInput {
    touch_enabled: bool,
    #[cfg(target_os = "espidf")]
    touch_handle: *mut c_void,
    #[cfg(target_os = "espidf")]
    logical_width: u16,
    #[cfg(target_os = "espidf")]
    logical_height: u16,
    #[cfg(target_os = "espidf")]
    rotation_degrees: u16,
    #[cfg(target_os = "espidf")]
    pointer_down: Cell<bool>,
    #[cfg(target_os = "espidf")]
    last_position: Cell<Option<(f32, f32)>>,
}

impl EmbeddedInput {
    pub fn is_touch_enabled(&self) -> bool {
        self.touch_enabled
    }

    #[cfg(target_os = "espidf")]
    pub fn pump_window_events(&self, window: &MinimalSoftwareWindow) -> Result<()> {
        if !self.touch_enabled || self.touch_handle.is_null() {
            return Ok(());
        }

        let err = unsafe { esp_lcd_touch_read_data(self.touch_handle) };
        if err != 0 {
            return Err(anyhow!("failed to read touch data: esp_err={err}"));
        }

        let mut x = [0u16; 1];
        let mut y = [0u16; 1];
        let mut strength = [0u16; 1];
        let mut point_count = 0u8;
        let touched = unsafe {
            esp_lcd_touch_get_coordinates(
                self.touch_handle,
                x.as_mut_ptr(),
                y.as_mut_ptr(),
                strength.as_mut_ptr(),
                &mut point_count,
                1,
            )
        };

        if touched && point_count > 0 {
            let position = self.map_touch_to_logical(x[0], y[0]);
            window
                .window()
                .try_dispatch_event(WindowEvent::PointerMoved { position })
                .map_err(|error| anyhow!("failed to dispatch touch move: {error}"))?;

            if !self.pointer_down.get() {
                window
                    .window()
                    .try_dispatch_event(WindowEvent::PointerPressed {
                        position,
                        button: PointerEventButton::Left,
                    })
                    .map_err(|error| anyhow!("failed to dispatch touch press: {error}"))?;
                self.pointer_down.set(true);
            }

            self.last_position.set(Some((position.x, position.y)));
            return Ok(());
        }

        if self.pointer_down.get() {
            let position = self
                .last_position
                .get()
                .map(|(x, y)| LogicalPosition::new(x, y))
                .unwrap_or_else(|| LogicalPosition::new(0., 0.));
            window
                .window()
                .try_dispatch_event(WindowEvent::PointerReleased {
                    position,
                    button: PointerEventButton::Left,
                })
                .map_err(|error| anyhow!("failed to dispatch touch release: {error}"))?;
            window
                .window()
                .try_dispatch_event(WindowEvent::PointerExited)
                .map_err(|error| anyhow!("failed to dispatch touch exit: {error}"))?;
            self.pointer_down.set(false);
        }

        Ok(())
    }

    #[cfg(target_os = "espidf")]
    fn map_touch_to_logical(&self, native_x: u16, native_y: u16) -> LogicalPosition {
        let logical_width = self.logical_width as f32;
        let logical_height = self.logical_height as f32;

        let (x, y) = match self.rotation_degrees {
            270 => (
                (self.logical_width.saturating_sub(1)
                    - native_y.min(self.logical_width.saturating_sub(1))) as f32,
                native_x.min(self.logical_height.saturating_sub(1)) as f32,
            ),
            90 => (
                native_y.min(self.logical_width.saturating_sub(1)) as f32,
                (self.logical_height.saturating_sub(1)
                    - native_x.min(self.logical_height.saturating_sub(1))) as f32,
            ),
            180 => (
                (self.logical_width.saturating_sub(1)
                    - native_x.min(self.logical_width.saturating_sub(1))) as f32,
                (self.logical_height.saturating_sub(1)
                    - native_y.min(self.logical_height.saturating_sub(1))) as f32,
            ),
            _ => (
                native_x.min(self.logical_width.saturating_sub(1)) as f32,
                native_y.min(self.logical_height.saturating_sub(1)) as f32,
            ),
        };

        LogicalPosition::new(
            x.clamp(0., logical_width - 1.),
            y.clamp(0., logical_height - 1.),
        )
    }
}

pub fn initialize_embedded_input(config: EmbeddedDisplayConfig) -> Result<EmbeddedInput> {
    #[cfg(target_os = "espidf")]
    {
        if !EAGER_TOUCH_BRINGUP_ENABLED {
            log::warn!(
                "embedded touch bring-up is temporarily deferred to avoid GSL3680 startup crashes during boot"
            );

            return Ok(EmbeddedInput {
                touch_enabled: false,
                touch_handle: core::ptr::null_mut(),
                logical_width: config.width,
                logical_height: config.height,
                rotation_degrees: config.rotation_degrees,
                pointer_down: Cell::new(false),
                last_position: Cell::new(None),
            });
        }

        let mut touch_handle = core::ptr::null_mut();
        let err = unsafe { bsp_touch_new(core::ptr::null(), &mut touch_handle) };
        let touch_enabled = if err == 0 {
            log::info!(
                "embedded touch initialized for logical {}x{} @ {} degrees",
                config.width,
                config.height,
                config.rotation_degrees
            );
            true
        } else {
            log::warn!(
                "embedded touch initialization failed; continuing without touch input: esp_err={err}"
            );
            false
        };

        return Ok(EmbeddedInput {
            touch_enabled,
            touch_handle,
            logical_width: config.width,
            logical_height: config.height,
            rotation_degrees: config.rotation_degrees,
            pointer_down: Cell::new(false),
            last_position: Cell::new(None),
        });
    }

    #[cfg(not(target_os = "espidf"))]
    {
        let _ = config;
        Ok(EmbeddedInput {
            touch_enabled: true,
        })
    }
}
