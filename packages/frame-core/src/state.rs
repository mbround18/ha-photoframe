use crate::control::ScreenStatus;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AppPhase {
    Splash,
    Setup,
    Ready,
}

impl AppPhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Splash => "Splash",
            Self::Setup => "Setup",
            Self::Ready => "Ready",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NetworkPhase {
    Unprovisioned,
    Provisioning,
    Connected,
}

impl NetworkPhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unprovisioned => "Unprovisioned",
            Self::Provisioning => "Provisioning",
            Self::Connected => "Connected",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ControllerPhase {
    NotStarted,
    Searching,
    AwaitingConfiguration,
    Connected,
    Error(String),
}

impl ControllerPhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NotStarted => "NotStarted",
            Self::Searching => "Searching",
            Self::AwaitingConfiguration => "AwaitingConfiguration",
            Self::Connected => "Connected",
            Self::Error(_) => "Error",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppState {
    pub phase: AppPhase,
    pub network_phase: NetworkPhase,
    pub controller_phase: ControllerPhase,
    pub active_media_url: Option<String>,
    pub screen_status: ScreenStatus,
    pub display_brightness: Option<u8>,
    pub provisioning_ssid: Option<String>,
    pub provisioning_password: Option<String>,
    pub device_id: Option<String>,
    pub device_name: Option<String>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            phase: AppPhase::Splash,
            network_phase: NetworkPhase::Unprovisioned,
            controller_phase: ControllerPhase::NotStarted,
            active_media_url: None,
            screen_status: ScreenStatus::Idle,
            display_brightness: None,
            provisioning_ssid: None,
            provisioning_password: None,
            device_id: None,
            device_name: None,
        }
    }

    pub fn begin_setup(&mut self) {
        self.phase = AppPhase::Setup;
    }

    pub fn mark_ready(&mut self) {
        self.phase = AppPhase::Ready;
    }

    pub fn set_network_phase(&mut self, phase: NetworkPhase) {
        if phase != NetworkPhase::Provisioning {
            self.clear_provisioning_details();
        }

        self.network_phase = phase;

        if self.network_phase != NetworkPhase::Connected {
            self.controller_phase = ControllerPhase::NotStarted;
        }
    }

    pub fn set_controller_phase(&mut self, phase: ControllerPhase) {
        self.controller_phase = phase;
    }

    pub fn set_controller_error(&mut self, error: impl Into<String>) {
        self.controller_phase = ControllerPhase::Error(error.into());
    }

    pub fn set_active_media_url(&mut self, media_url: impl Into<String>) {
        self.active_media_url = Some(media_url.into());
    }

    pub fn clear_active_media_url(&mut self) {
        self.active_media_url = None;
    }

    pub fn set_screen_status(&mut self, screen_status: ScreenStatus) {
        self.screen_status = screen_status;
    }

    pub fn set_display_brightness(&mut self, brightness: Option<u8>) {
        self.display_brightness = brightness;
    }

    pub fn set_provisioning_details(&mut self, ssid: String, password: String) {
        self.provisioning_ssid = Some(ssid);
        self.provisioning_password = Some(password);
    }

    pub fn clear_provisioning_details(&mut self) {
        self.provisioning_ssid = None;
        self.provisioning_password = None;
    }

    pub fn set_device_identity(
        &mut self,
        device_id: impl Into<String>,
        device_name: impl Into<String>,
    ) {
        self.device_id = Some(device_id.into());
        self.device_name = Some(device_name.into());
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{AppPhase, AppState, ControllerPhase, NetworkPhase};

    #[test]
    fn app_state_transitions_from_splash_to_ready() {
        let mut state = AppState::new();

        assert_eq!(state.phase, AppPhase::Splash);
        assert_eq!(state.network_phase, NetworkPhase::Unprovisioned);

        state.begin_setup();
        state.set_network_phase(NetworkPhase::Connected);
        state.mark_ready();

        assert_eq!(state.phase, AppPhase::Ready);
        assert_eq!(state.network_phase, NetworkPhase::Connected);
    }

    #[test]
    fn leaving_provisioning_clears_the_wifi_credential() {
        let mut state = AppState::new();

        state.set_provisioning_details("Frame Setup".to_string(), "secret".to_string());
        state.set_network_phase(NetworkPhase::Provisioning);
        assert_eq!(state.provisioning_ssid.as_deref(), Some("Frame Setup"));

        // The Wi-Fi password must not linger in memory once provisioning ends
        // (Principle II: the frame keeps only what it still needs).
        state.set_network_phase(NetworkPhase::Connected);
        assert_eq!(state.provisioning_ssid, None);
        assert_eq!(state.provisioning_password, None);
    }

    #[test]
    fn losing_the_network_resets_the_controller_phase() {
        let mut state = AppState::new();

        state.set_network_phase(NetworkPhase::Connected);
        state.set_controller_phase(ControllerPhase::Connected);
        assert_eq!(state.controller_phase, ControllerPhase::Connected);

        state.set_network_phase(NetworkPhase::Unprovisioned);
        assert_eq!(state.controller_phase, ControllerPhase::NotStarted);
    }
}
