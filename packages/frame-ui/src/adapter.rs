use anyhow::Result;
use frame_core::AppState;

#[cfg(not(target_os = "espidf"))]
use slint::ComponentHandle;

#[cfg(target_os = "espidf")]
use std::ffi::CString;

#[cfg(target_os = "espidf")]
use anyhow::anyhow;

#[cfg(not(target_os = "espidf"))]
use crate::MainWindow;

pub trait UiAdapter {
    fn sync_state(&mut self, state: &AppState) -> Result<()>;
    fn run(&self) -> Result<()>;
}

pub fn create_ui() -> Result<Box<dyn UiAdapter>> {
    #[cfg(target_os = "espidf")]
    {
        return Ok(Box::new(FirmwareUiAdapter::new()?));
    }

    #[cfg(not(target_os = "espidf"))]
    {
        Ok(Box::new(SlintUiAdapter::new()?))
    }
}

fn headline_text(state: &AppState) -> &'static str {
    match state.phase {
        frame_core::AppPhase::Splash => "Welcome",
        frame_core::AppPhase::Setup => "Setup",
        frame_core::AppPhase::Ready => "Photo Frame Ready",
    }
}

fn status_text(state: &AppState) -> &'static str {
    match state.phase {
        frame_core::AppPhase::Splash => "Starting the photo frame",
        frame_core::AppPhase::Setup => "Finishing first-time setup",
        frame_core::AppPhase::Ready => "Ready to show your photos",
    }
}

fn detail_text(state: &AppState) -> &'static str {
    match (state.phase.clone(), state.network_phase.clone()) {
        (frame_core::AppPhase::Splash, _) => {
            "Bringing up the display and preparing the setup flow."
        }
        (frame_core::AppPhase::Setup, frame_core::NetworkPhase::Provisioning) => {
            "Connect the frame to Wi-Fi to continue setup."
        }
        (frame_core::AppPhase::Setup, frame_core::NetworkPhase::Unprovisioned) => {
            "Waiting for network provisioning to begin."
        }
        (frame_core::AppPhase::Setup, frame_core::NetworkPhase::Authorizing) => {
            "Connect your Google account to continue."
        }
        (frame_core::AppPhase::Setup, frame_core::NetworkPhase::Connected) => {
            "Network is ready. Finishing setup now."
        }
        (frame_core::AppPhase::Ready, _) => "Network is up and the photo pipeline is ready.",
    }
}

#[cfg(not(target_os = "espidf"))]
struct SlintUiAdapter {
    window: MainWindow,
}

#[cfg(not(target_os = "espidf"))]
impl SlintUiAdapter {
    fn new() -> Result<Self> {
        let window = MainWindow::new()?;
        let weak_window = window.as_weak();

        window.on_connect_to_wifi(move || {
            if let Some(window) = weak_window.upgrade() {
                use slint::Model;
                let networks = frame_net::wifi::scan().unwrap_or_default();
                let ui_networks: Vec<crate::WifiNetwork> = networks
                    .into_iter()
                    .map(|net| crate::WifiNetwork { ssid: net.ssid.into() })
                    .collect();
                let model = std::rc::Rc::new(slint::VecModel::from(ui_networks));
                window.set_wifi_networks(model.into());
            }
        });

        window.on_network_selected(move |ssid| {
            tracing::info!("Network selected: {}", ssid);
        });

        window.on_password_submitted(move |ssid, password| {
            tracing::info!("Password submitted for SSID '{}': '{}'", ssid, password);
            match frame_net::wifi::connect(&ssid, &password) {
                Ok(_) => tracing::info!("successfully connected to wifi"),
                Err(e) => tracing::error!("failed to connect to wifi: {}", e),
            }
        });

        Ok(Self { window })
    }
}

#[cfg(not(target_os = "espidf"))]
impl UiAdapter for SlintUiAdapter {
    fn sync_state(&mut self, state: &AppState) -> Result<()> {
        self.window.set_headline_text(headline_text(state).into());
        self.window.set_status_text(status_text(state).into());
        self.window
            .set_network_status(state.network_phase.as_str().into());
        self.window.set_detail_text(detail_text(state).into());
        self.window.set_auth_user_code(
            state
                .auth_user_code
                .as_deref()
                .unwrap_or_default()
                .to_string()
                .into(),
        );
        self.window.set_auth_verification_uri(
            state
                .auth_verification_uri
                .as_deref()
                .unwrap_or_default()
                .to_string()
                .into(),
        );

        Ok(())
    }

    fn run(&self) -> Result<()> {
        self.window.run()?;
        Ok(())
    }
}

#[cfg(target_os = "espidf")]
unsafe extern "C" {
    fn frame_embedded_ui_start() -> i32;
    fn frame_embedded_ui_sync(
        headline: *const core::ffi::c_char,
        status: *const core::ffi::c_char,
        network: *const core::ffi::c_char,
        detail: *const core::ffi::c_char,
    ) -> i32;
}

#[cfg(target_os = "espidf")]
struct FirmwareUiAdapter;

#[cfg(target_os = "espidf")]
impl FirmwareUiAdapter {
    fn new() -> Result<Self> {
        let err = unsafe { frame_embedded_ui_start() };
        if err != 0 {
            return Err(anyhow!("failed to start embedded UI: esp_err={err}"));
        }

        Ok(Self)
    }
}

#[cfg(target_os = "espidf")]
impl UiAdapter for FirmwareUiAdapter {
    fn sync_state(&mut self, state: &AppState) -> Result<()> {
        let headline = CString::new(headline_text(state))?;
        let status = CString::new(status_text(state))?;
        let network = CString::new(state.network_phase.as_str())?;
        let detail = CString::new(detail_text(state))?;
        let err = unsafe {
            frame_embedded_ui_sync(
                headline.as_ptr(),
                status.as_ptr(),
                network.as_ptr(),
                detail.as_ptr(),
            )
        };
        if err != 0 {
            return Err(anyhow!("failed to sync embedded UI: esp_err={err}"));
        }

        Ok(())
    }

    fn run(&self) -> Result<()> {
        loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    }
}
