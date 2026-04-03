use crate::models::{AlbumMetadata, GoogleUser, PhotoMetadata};

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
    pub photos: Vec<PhotoMetadata>,
    pub albums: Vec<AlbumMetadata>,
    pub current_album: Option<AlbumMetadata>,
    pub google_user: Option<GoogleUser>,
    pub access_token: Option<String>,
    pub auth_user_code: Option<String>,
    pub auth_verification_uri: Option<String>,
    pub provisioning_ssid: Option<String>,
    pub provisioning_password: Option<String>,
    pub local_setup_host: Option<String>,
    pub local_setup_url: Option<String>,
    pub local_setup_ip_url: Option<String>,
    pub pairing_code: Option<String>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            phase: AppPhase::Splash,
            network_phase: NetworkPhase::Unprovisioned,
            photos: Vec::new(),
            albums: Vec::new(),
            current_album: None,
            google_user: None,
            access_token: None,
            auth_user_code: None,
            auth_verification_uri: None,
            provisioning_ssid: None,
            provisioning_password: None,
            local_setup_host: None,
            local_setup_url: None,
            local_setup_ip_url: None,
            pairing_code: None,
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

        if phase != NetworkPhase::Authorizing {
            self.clear_auth_info();
        }

        self.network_phase = phase;
    }

    pub fn set_google_user(&mut self, google_user: GoogleUser) {
        self.google_user = Some(google_user);
    }

    pub fn set_access_token(&mut self, access_token: impl Into<String>) {
        self.access_token = Some(access_token.into());
    }

    pub fn clear_access_token(&mut self) {
        self.access_token = None;
    }

    pub fn clear_google_user(&mut self) {
        self.google_user = None;
    }

    pub fn google_user_email(&self) -> Option<&str> {
        self.google_user
            .as_ref()
            .map(|google_user| google_user.email.as_str())
    }

    pub fn google_user_subject(&self) -> Option<&str> {
        self.google_user
            .as_ref()
            .map(|google_user| google_user.subject.as_str())
    }

    pub fn set_auth_info(&mut self, user_code: String, verification_uri: String) {
        self.auth_user_code = Some(user_code);
        self.auth_verification_uri = Some(verification_uri);
    }

    pub fn set_provisioning_details(&mut self, ssid: String, password: String) {
        self.provisioning_ssid = Some(ssid);
        self.provisioning_password = Some(password);
    }

    pub fn clear_provisioning_details(&mut self) {
        self.provisioning_ssid = None;
        self.provisioning_password = None;
    }

    pub fn set_local_setup_details(
        &mut self,
        host: impl Into<String>,
        url: Option<String>,
        ip_url: Option<String>,
    ) {
        self.local_setup_host = Some(host.into());
        self.local_setup_url = url;
        self.local_setup_ip_url = ip_url;
    }

    pub fn set_pairing_code(&mut self, pairing_code: impl Into<String>) {
        self.pairing_code = Some(pairing_code.into());
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
    use crate::models::GoogleUser;

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
    fn network_phase_transition_clears_stale_setup_data() {
        let mut state = AppState::new();

        state.set_provisioning_details("Frame Setup".to_string(), "secret".to_string());
        state.set_auth_info("ABCD-12".to_string(), "google.com/device".to_string());

        state.set_network_phase(NetworkPhase::Provisioning);
        assert_eq!(state.provisioning_ssid.as_deref(), Some("Frame Setup"));
        assert_eq!(state.auth_user_code, None);

        state.set_auth_info("ABCD-12".to_string(), "google.com/device".to_string());
        state.set_network_phase(NetworkPhase::Authorizing);
        assert_eq!(state.provisioning_ssid, None);
        assert_eq!(state.auth_user_code.as_deref(), Some("ABCD-12"));

        state.set_network_phase(NetworkPhase::Connected);
        assert_eq!(state.provisioning_ssid, None);
        assert_eq!(state.provisioning_password, None);
        assert_eq!(state.auth_user_code, None);
        assert_eq!(state.auth_verification_uri, None);
    }

    #[test]
    fn app_state_tracks_google_user_profile() {
        let mut state = AppState::new();
        let google_user = GoogleUser {
            email: "owner@example.com".to_string(),
            subject: "owner-subject".to_string(),
            refresh_token: "refresh-token".to_string(),
        };

        state.set_google_user(google_user.clone());

        assert_eq!(state.google_user, Some(google_user));
        assert_eq!(state.google_user_email(), Some("owner@example.com"));
        assert_eq!(state.google_user_subject(), Some("owner-subject"));

        state.clear_google_user();

        assert_eq!(state.google_user, None);
        assert_eq!(state.google_user_email(), None);
        assert_eq!(state.google_user_subject(), None);
    }
}
