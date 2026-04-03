#[cfg(target_os = "espidf")]
use anyhow::{Context, Result, anyhow};
#[cfg(target_os = "espidf")]
use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspKeyValueStorage, EspNvs};
#[cfg(target_os = "espidf")]
use frame_core::{AppPhase, AppState, NetworkPhase};

#[cfg(target_os = "espidf")]
const SETUP_STATE_NAMESPACE: &str = "frame_state";
#[cfg(target_os = "espidf")]
const KEY_SETUP_STATE: &str = "checkpoint";
#[cfg(target_os = "espidf")]
const STATE_MAGIC: [u8; 4] = *b"FSTS";
#[cfg(target_os = "espidf")]
const STATE_VERSION: u8 = 1;
#[cfg(target_os = "espidf")]
const STATE_RECORD_LEN: usize = 9;

#[cfg(target_os = "espidf")]
const FLAG_OWNER_PRESENT: u8 = 1 << 0;
#[cfg(target_os = "espidf")]
const FLAG_PAIRING_CODE_PRESENT: u8 = 1 << 1;
#[cfg(target_os = "espidf")]
const FLAG_BROWSER_VERIFIED: u8 = 1 << 2;
#[cfg(target_os = "espidf")]
const FLAG_AUTH_CODE_PRESENT: u8 = 1 << 3;
#[cfg(target_os = "espidf")]
const FLAG_LOCAL_SETUP_URL_PRESENT: u8 = 1 << 4;

#[cfg(target_os = "espidf")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum SetupCheckpoint {
    BootStarted = 1,
    SplashRendered = 2,
    SetupRendered = 3,
    OwnerRestored = 4,
    Provisioning = 5,
    NetworkConnected = 6,
    LocalSetupReady = 7,
    AwaitingBrowserPair = 8,
    BrowserPairVerified = 9,
    DeviceCodeReady = 10,
    AuthorizationComplete = 11,
    Ready = 12,
}

#[cfg(target_os = "espidf")]
impl SetupCheckpoint {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BootStarted => "boot-started",
            Self::SplashRendered => "splash-rendered",
            Self::SetupRendered => "setup-rendered",
            Self::OwnerRestored => "owner-restored",
            Self::Provisioning => "provisioning",
            Self::NetworkConnected => "network-connected",
            Self::LocalSetupReady => "local-setup-ready",
            Self::AwaitingBrowserPair => "awaiting-browser-pair",
            Self::BrowserPairVerified => "browser-pair-verified",
            Self::DeviceCodeReady => "device-code-ready",
            Self::AuthorizationComplete => "authorization-complete",
            Self::Ready => "ready",
        }
    }

    fn from_byte(value: u8) -> Option<Self> {
        Some(match value {
            1 => Self::BootStarted,
            2 => Self::SplashRendered,
            3 => Self::SetupRendered,
            4 => Self::OwnerRestored,
            5 => Self::Provisioning,
            6 => Self::NetworkConnected,
            7 => Self::LocalSetupReady,
            8 => Self::AwaitingBrowserPair,
            9 => Self::BrowserPairVerified,
            10 => Self::DeviceCodeReady,
            11 => Self::AuthorizationComplete,
            12 => Self::Ready,
            _ => return None,
        })
    }
}

#[cfg(target_os = "espidf")]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistedSetupState {
    pub checkpoint: SetupCheckpoint,
    pub app_phase: AppPhase,
    pub network_phase: NetworkPhase,
    pub flags: u8,
}

#[cfg(target_os = "espidf")]
impl PersistedSetupState {
    pub fn from_app_state(
        checkpoint: SetupCheckpoint,
        app_state: &AppState,
        browser_verified: bool,
    ) -> Self {
        let mut flags = 0_u8;

        if app_state.google_user.is_some() {
            flags |= FLAG_OWNER_PRESENT;
        }
        if app_state.pairing_code.as_deref().is_some_and(|value| !value.is_empty()) {
            flags |= FLAG_PAIRING_CODE_PRESENT;
        }
        if browser_verified {
            flags |= FLAG_BROWSER_VERIFIED;
        }
        if app_state.auth_user_code.as_deref().is_some_and(|value| !value.is_empty()) {
            flags |= FLAG_AUTH_CODE_PRESENT;
        }
        if app_state
            .local_setup_ip_url
            .as_deref()
            .or(app_state.local_setup_url.as_deref())
            .is_some_and(|value| !value.is_empty())
        {
            flags |= FLAG_LOCAL_SETUP_URL_PRESENT;
        }

        Self {
            checkpoint,
            app_phase: app_state.phase.clone(),
            network_phase: app_state.network_phase.clone(),
            flags,
        }
    }

    pub fn flags_summary(&self) -> String {
        let mut labels = Vec::new();

        if self.flags & FLAG_OWNER_PRESENT != 0 {
            labels.push("owner");
        }
        if self.flags & FLAG_PAIRING_CODE_PRESENT != 0 {
            labels.push("pairing-code");
        }
        if self.flags & FLAG_BROWSER_VERIFIED != 0 {
            labels.push("browser-verified");
        }
        if self.flags & FLAG_AUTH_CODE_PRESENT != 0 {
            labels.push("auth-code");
        }
        if self.flags & FLAG_LOCAL_SETUP_URL_PRESENT != 0 {
            labels.push("local-url");
        }

        if labels.is_empty() {
            "none".to_string()
        } else {
            labels.join(",")
        }
    }

    fn encode(&self) -> [u8; STATE_RECORD_LEN] {
        [
            STATE_MAGIC[0],
            STATE_MAGIC[1],
            STATE_MAGIC[2],
            STATE_MAGIC[3],
            STATE_VERSION,
            self.checkpoint as u8,
            encode_app_phase(&self.app_phase),
            encode_network_phase(&self.network_phase),
            self.flags,
        ]
    }

    fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != STATE_RECORD_LEN {
            anyhow::bail!("unexpected setup state length: {}", bytes.len());
        }

        if bytes[0..4] != STATE_MAGIC {
            anyhow::bail!("unexpected setup state magic");
        }

        if bytes[4] != STATE_VERSION {
            anyhow::bail!("unsupported setup state version: {}", bytes[4]);
        }

        Ok(Self {
            checkpoint: SetupCheckpoint::from_byte(bytes[5])
                .ok_or_else(|| anyhow!("invalid setup checkpoint byte: {}", bytes[5]))?,
            app_phase: decode_app_phase(bytes[6])
                .ok_or_else(|| anyhow!("invalid app phase byte: {}", bytes[6]))?,
            network_phase: decode_network_phase(bytes[7])
                .ok_or_else(|| anyhow!("invalid network phase byte: {}", bytes[7]))?,
            flags: bytes[8],
        })
    }
}

#[cfg(target_os = "espidf")]
pub struct SetupStateStore {
    storage: EspKeyValueStorage<esp_idf_svc::nvs::NvsDefault>,
}

#[cfg(target_os = "espidf")]
impl SetupStateStore {
    pub fn new(partition: EspDefaultNvsPartition) -> Result<Self> {
        let nvs = EspNvs::new(partition, SETUP_STATE_NAMESPACE, true)
            .context("failed to open setup state namespace")?;

        Ok(Self {
            storage: EspKeyValueStorage::new(nvs),
        })
    }

    pub fn load(&self) -> Result<Option<PersistedSetupState>> {
        let mut buffer = [0_u8; STATE_RECORD_LEN];
        let Some(raw) = self
            .storage
            .get_raw(KEY_SETUP_STATE, &mut buffer)
            .context("failed to load setup checkpoint blob")?
        else {
            return Ok(None);
        };

        PersistedSetupState::decode(raw).map(Some)
    }

    pub fn save_checkpoint(
        &self,
        checkpoint: SetupCheckpoint,
        app_state: &AppState,
        browser_verified: bool,
    ) -> Result<PersistedSetupState> {
        let snapshot = PersistedSetupState::from_app_state(checkpoint, app_state, browser_verified);
        self.storage
            .set_raw(KEY_SETUP_STATE, &snapshot.encode())
            .context("failed to persist setup checkpoint blob")?;
        Ok(snapshot)
    }
}

#[cfg(target_os = "espidf")]
fn encode_app_phase(phase: &AppPhase) -> u8 {
    match phase {
        AppPhase::Splash => 1,
        AppPhase::Setup => 2,
        AppPhase::Ready => 3,
    }
}

#[cfg(target_os = "espidf")]
fn decode_app_phase(value: u8) -> Option<AppPhase> {
    Some(match value {
        1 => AppPhase::Splash,
        2 => AppPhase::Setup,
        3 => AppPhase::Ready,
        _ => return None,
    })
}

#[cfg(target_os = "espidf")]
fn encode_network_phase(phase: &NetworkPhase) -> u8 {
    match phase {
        NetworkPhase::Unprovisioned => 1,
        NetworkPhase::Provisioning => 2,
        NetworkPhase::Authorizing => 3,
        NetworkPhase::Connected => 4,
    }
}

#[cfg(target_os = "espidf")]
fn decode_network_phase(value: u8) -> Option<NetworkPhase> {
    Some(match value {
        1 => NetworkPhase::Unprovisioned,
        2 => NetworkPhase::Provisioning,
        3 => NetworkPhase::Authorizing,
        4 => NetworkPhase::Connected,
        _ => return None,
    })
}