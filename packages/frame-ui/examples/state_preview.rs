use anyhow::Result;
use frame_core::{AppPhase, AppState, NetworkPhase};
use frame_ui::{MainWindow, sync_window_state};
use slint::ComponentHandle;

fn preview_state(screen: &str) -> AppState {
    let mut state = AppState::new();

    match screen {
        "wifi-connect" => {
            state.begin_setup();
            state.set_network_phase(NetworkPhase::Provisioning);
            state.set_provisioning_details("Frame Setup 4821".to_string(), String::new());
        }
        "ha-syncing" => {
            state.begin_setup();
            state.set_network_phase(NetworkPhase::Connected);
            state.mark_ready();
            state.set_controller_phase(frame_core::ControllerPhase::Searching);
        }
        "ready" => {
            state.begin_setup();
            state.set_network_phase(NetworkPhase::Connected);
            state.mark_ready();
        }
        _ => {
            state.phase = AppPhase::Splash;
            state.set_network_phase(NetworkPhase::Unprovisioned);
        }
    }

    state
}

fn apply_preview(window: &MainWindow, screen: &str) {
    let state = preview_state(screen);
    window.set_screen_override(screen.into());
    sync_window_state(window, &state);
}

fn main() -> Result<()> {
    let window = MainWindow::new()?;
    window.set_preview_mode(true);

    apply_preview(&window, "welcome");

    let weak = window.as_weak();
    window.on_preview_select(move |screen| {
        if let Some(window) = weak.upgrade() {
            apply_preview(&window, screen.as_str());
        }
    });

    window.run()?;
    Ok(())
}
