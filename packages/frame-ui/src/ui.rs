//! Drives what the panel shows.
//!
//! The frame has exactly one job once adopted: show the current photo. Before
//! that it shows whichever setup screen matches where it has got to. This type
//! owns the panel and decides between those two worlds.

#![cfg(target_os = "espidf")]

use anyhow::Result;
use frame_core::{AppState, ControllerPhase, NetworkPhase};

use crate::controller_state::controller_state_snapshot;
use crate::panel::Panel;
use crate::rendered_image::rendered_image_snapshot;
use crate::screens;

/// What the panel is currently showing, so we only redraw on a real change.
/// Redrawing a static screen wastes a 2 MB rotate and a DMA transfer per pass.
#[derive(Clone, Debug, Eq, PartialEq)]
enum Showing {
    Nothing,
    Starting,
    WifiSetup(String),
    AwaitingAdoption(String),
    Ready,
    Photo(u64),
}

pub struct FrameUi {
    panel: Panel,
    showing: Showing,
}

pub fn create_ui() -> Result<FrameUi> {
    // The colour sweep confirmed solid fills render clean on this panel, so the
    // diagnostic is retired. `Panel::diagnostic_colour_sweep` is kept for the
    // next time a display fault needs isolating from our drawing code.
    Ok(FrameUi {
        panel: Panel::init()?,
        showing: Showing::Nothing,
    })
}

impl FrameUi {
    /// Reconcile the panel with the current state. Cheap to call in a loop.
    pub fn sync(&mut self, state: &AppState) -> Result<()> {
        // A photo always wins: once Home Assistant is sending pictures, the
        // panel shows pictures and nothing else (Principle VIII).
        let rendered = rendered_image_snapshot()?;
        if let Some(image) = rendered.image.as_ref() {
            let wanted = Showing::Photo(rendered.generation);
            if self.showing != wanted {
                self.panel.present(image.rgb565())?;
                self.showing = wanted;
            }
            return Ok(());
        }

        let controller = controller_state_snapshot()?;
        let wanted = match (&state.network_phase, &controller.phase) {
            (_, ControllerPhase::Connected) => Showing::Ready,
            (NetworkPhase::Provisioning, _) => Showing::WifiSetup(
                state
                    .provisioning_ssid
                    .clone()
                    .unwrap_or_else(|| "Photo Frame setup".to_string()),
            ),
            (NetworkPhase::Connected, _) => Showing::AwaitingAdoption(
                state
                    .device_id
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string()),
            ),
            _ => Showing::Starting,
        };

        if self.showing == wanted {
            return Ok(());
        }

        match &wanted {
            Showing::Starting => screens::show_starting(&mut self.panel)?,
            Showing::WifiSetup(ssid) => screens::show_wifi_setup(&mut self.panel, ssid)?,
            Showing::AwaitingAdoption(id) => screens::show_awaiting_adoption(&mut self.panel, id)?,
            Showing::Ready => screens::show_ready(&mut self.panel)?,
            Showing::Nothing | Showing::Photo(_) => {}
        }
        self.showing = wanted;
        Ok(())
    }

    pub fn set_brightness(&self, percent: u8) -> Result<()> {
        self.panel.set_backlight(percent)
    }
}
