use crate::models::PhotoMetadata;

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
    Authorizing,
    Connected,
}

impl NetworkPhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unprovisioned => "Unprovisioned",
            Self::Provisioning => "Provisioning",
            Self::Authorizing => "Authorizing",
            Self::Connected => "Connected",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppState {
    pub phase: AppPhase,
    pub network_phase: NetworkPhase,
    pub current_photo: Option<PhotoMetadata>,
    pub auth_user_code: Option<String>,
    pub auth_verification_uri: Option<String>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            phase: AppPhase::Splash,
            network_phase: NetworkPhase::Unprovisioned,
            current_photo: None,
            auth_user_code: None,
            auth_verification_uri: None,
        }
    }

    pub fn begin_setup(&mut self) {
        self.phase = AppPhase::Setup;
    }

    pub fn mark_ready(&mut self) {
        self.phase = AppPhase::Ready;
    }

    pub fn set_network_phase(&mut self, phase: NetworkPhase) {
        self.network_phase = phase;
    }

    pub fn set_auth_info(&mut self, user_code: String, verification_uri: String) {
        self.auth_user_code = Some(user_code);
        self.auth_verification_uri = Some(verification_uri);
    }

    pub fn clear_auth_info(&mut self) {
        self.auth_user_code = None;
        self.auth_verification_uri = None;
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{AppPhase, AppState, NetworkPhase};

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
}
