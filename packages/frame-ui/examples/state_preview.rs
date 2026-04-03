use anyhow::Result;
use frame_core::models::GoogleUser;
use frame_core::{AppPhase, AppState, NetworkPhase};
use frame_ui::{sync_window_state, MainWindow};
use slint::ComponentHandle;

fn preview_state(screen: &str) -> AppState {
    let mut state = AppState::new();

    match screen {
        "wifi-connect" => {
            state.begin_setup();
            state.set_network_phase(NetworkPhase::Provisioning);
            state.set_provisioning_details("Frame Setup 4821".to_string(), String::new());
        }
        "browser-pairing" => {
            state.begin_setup();
            state.set_network_phase(NetworkPhase::Connected);
            state.set_local_setup_details(
                "frame.local",
                Some("http://frame.local".to_string()),
                Some("http://192.168.1.44".to_string()),
            );
            state.set_pairing_code("482731");
        }
        "pairing" => {
            state.begin_setup();
            state.set_network_phase(NetworkPhase::Authorizing);
            state.set_local_setup_details(
                "frame.local",
                Some("http://frame.local".to_string()),
                Some("http://192.168.1.44".to_string()),
            );
            state.set_pairing_code("482731");
            state.set_auth_info("M7QX-2L".to_string(), "google.com/device".to_string());
        }
        "ready" => {
            state.begin_setup();
            state.set_network_phase(NetworkPhase::Connected);
            state.set_google_user(GoogleUser {
                email: "maria@example.com".to_string(),
                subject: "owner-subject".to_string(),
                refresh_token: "sample-token".to_string(),
            });
            state.set_local_setup_details(
                "frame.local",
                Some("http://frame.local".to_string()),
                Some("http://192.168.1.44".to_string()),
            );
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
