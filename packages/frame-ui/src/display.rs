use anyhow::Result;
use slint::platform::software_renderer::Rgb565Pixel;
use std::cell::{Cell, RefCell, RefMut};

#[cfg(target_os = "espidf")]
use anyhow::anyhow;

#[cfg(target_os = "espidf")]
use esp_idf_hal::task::do_yield;

#[cfg(target_os = "espidf")]
unsafe extern "C" {
    fn frame_embedded_panel_init(width: u16, height: u16, rotation_degrees: u16) -> i32;
    fn frame_embedded_panel_present(pixels: *const u16, width: u16, height: u16) -> i32;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EmbeddedDisplayConfig {
    pub width: u16,
    pub height: u16,
    pub rotation_degrees: u16,
}

impl Default for EmbeddedDisplayConfig {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 800,
            rotation_degrees: 270,
        }
    }
}

pub struct EmbeddedDisplay {
    config: EmbeddedDisplayConfig,
    framebuffer: RefCell<Vec<Rgb565Pixel>>,
    presented_frames: Cell<u64>,
}

impl EmbeddedDisplay {
    pub fn config(&self) -> EmbeddedDisplayConfig {
        self.config
    }

    pub fn framebuffer(&self) -> RefMut<'_, Vec<Rgb565Pixel>> {
        self.framebuffer.borrow_mut()
    }

    pub fn present(&self) -> Result<()> {
        #[cfg(target_os = "espidf")]
        {
            embedded_watchdog_tick();
            do_yield();

            let framebuffer = self.framebuffer.borrow();
            let err = unsafe {
                frame_embedded_panel_present(
                    framebuffer.as_ptr().cast::<u16>(),
                    self.config.width,
                    self.config.height,
                )
            };
            if err != 0 {
                return Err(anyhow!(
                    "failed to present embedded panel frame: esp_err={err}"
                ));
            }

            embedded_watchdog_tick();
            do_yield();
        }

        let presented = self.presented_frames.get() + 1;
        self.presented_frames.set(presented);

        if presented == 1 || presented % 120 == 0 {
            log::info!(
                "embedded Slint frame buffer updated: {} frames at {}x{}",
                presented,
                self.config.width,
                self.config.height
            );
        }

        Ok(())
    }

    pub fn presented_frames(&self) -> u64 {
        self.presented_frames.get()
    }
}

#[cfg(target_os = "espidf")]
fn embedded_watchdog_tick() {
    unsafe {
        if esp_idf_sys::esp_task_wdt_status(core::ptr::null_mut()) == esp_idf_sys::ESP_OK {
            let _ = esp_idf_sys::esp_task_wdt_reset();
        }
    }
}

pub fn initialize_embedded_display() -> Result<EmbeddedDisplay> {
    let config = EmbeddedDisplayConfig::default();
    let pixel_count = usize::from(config.width) * usize::from(config.height);

    #[cfg(target_os = "espidf")]
    {
        let err = unsafe {
            frame_embedded_panel_init(config.width, config.height, config.rotation_degrees)
        };
        if err != 0 {
            return Err(anyhow!(
                "failed to initialize embedded panel bridge: esp_err={err}"
            ));
        }
    }

    Ok(EmbeddedDisplay {
        config,
        framebuffer: RefCell::new(vec![Rgb565Pixel::default(); pixel_count]),
        presented_frames: Cell::new(0),
    })
}
