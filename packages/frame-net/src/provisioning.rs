use frame_core::NetworkPhase;

#[cfg(target_os = "espidf")]
use crate::wifi;
#[cfg(target_os = "espidf")]
use anyhow::anyhow;
#[cfg(target_os = "espidf")]
use frame_captive_portal::{self, ConnectState, WifiNetwork};
#[cfg(target_os = "espidf")]
use rand::{thread_rng, Rng};

pub trait ProvisioningManager {
    fn ensure_network(&mut self) -> anyhow::Result<NetworkPhase>;
    fn get_provisioning_ap_details(&self) -> Option<(String, String)>;
}

#[derive(Default)]
pub struct HostProvisioningManager;

impl ProvisioningManager for HostProvisioningManager {
    fn ensure_network(&mut self) -> anyhow::Result<NetworkPhase> {
        Ok(NetworkPhase::Connected)
    }

    fn get_provisioning_ap_details(&self) -> Option<(String, String)> {
        None
    }
}

#[cfg(target_os = "espidf")]
#[derive(Clone)]
enum ProvisioningState {
    NotStarted,
    Started(String, String),
    Finished(String, String),
}

#[cfg(target_os = "espidf")]
pub struct EspProvisioningManager {
    state: ProvisioningState,
    captive_portal: frame_captive_portal::CaptivePortal,
}

#[cfg(target_os = "espidf")]
impl Default for EspProvisioningManager {
    fn default() -> Self {
        Self {
            state: ProvisioningState::NotStarted,
            captive_portal: frame_captive_portal::create_captive_portal().unwrap(),
        }
    }
}

#[cfg(target_os = "espidf")]
fn embedded_or_runtime_env(key: &str, baked: Option<&'static str>) -> Option<String> {
    std::env::var(key)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            baked
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        })
}

#[cfg(target_os = "espidf")]
fn configured_station_credentials() -> Option<(String, String)> {
    let ssid = embedded_or_runtime_env("WIFI_SSID", option_env!("WIFI_SSID"))?;
    let password =
        embedded_or_runtime_env("WIFI_PASSWORD", option_env!("WIFI_PASSWORD")).unwrap_or_default();
    Some((ssid, password))
}

#[cfg(target_os = "espidf")]
fn generate_ap_ssid() -> String {
    let mut rng = thread_rng();
    let suffix: u16 = rng.gen_range(0..=0xFFFF);
    format!("Frame-{suffix:04X}")
}

#[cfg(target_os = "espidf")]
fn generate_ap_password() -> String {
    String::new()
}

#[cfg(target_os = "espidf")]
impl ProvisioningManager for EspProvisioningManager {
    fn ensure_network(&mut self) -> anyhow::Result<NetworkPhase> {
        match self.state.clone() {
            ProvisioningState::NotStarted => {
                if let Some((ssid, password)) = configured_station_credentials() {
                    log::info!(
                        "WIFI_SSID is configured; attempting direct station connection to '{ssid}' before starting provisioning AP"
                    );

                    match wifi::connect(&ssid, &password) {
                        Ok(()) => {
                            self.state = ProvisioningState::Finished(ssid, password);
                            return Ok(NetworkPhase::Connected);
                        }
                        Err(error) => {
                            log::warn!(
                                "direct station connection from embedded Wi-Fi credentials failed: {error}; falling back to provisioning AP"
                            );
                        }
                    }
                }

                let ap_ssid = generate_ap_ssid();
                let ap_password = generate_ap_password();

                log::info!(
                    "starting provisioning AP request: ssid='{ap_ssid}', password_len={}, protected={}",
                    ap_password.len(),
                    !ap_password.is_empty()
                );

                self.captive_portal.set_scan_handler(|| {
                    let aps = wifi::scan()?;
                    let networks = aps
                        .into_iter()
                        .map(|ap| WifiNetwork { ssid: ap.ssid })
                        .collect();
                    Ok(networks)
                })?;

                wifi::start_ap(&ap_ssid, &ap_password)?;
                self.captive_portal.start()?;

                let (effective_ssid, effective_password) = wifi::current_ap_credentials()?
                    .ok_or_else(|| {
                        anyhow!("ap started but effective credentials are unavailable")
                    })?;

                log::info!(
                    "provisioning AP active: ssid='{effective_ssid}', password_len={}",
                    effective_password.len()
                );

                if effective_ssid != ap_ssid {
                    log::warn!(
                        "requested AP ssid '{ap_ssid}' but active AP ssid is '{effective_ssid}'"
                    );
                }

                if effective_password.is_empty() {
                    log::warn!(
                        "provisioning AP '{effective_ssid}' is running as an open network to match current hosted-stack behavior"
                    );
                }

                self.state = ProvisioningState::Started(effective_ssid, effective_password);
                Ok(NetworkPhase::Provisioning)
            }
            ProvisioningState::Started(ap_ssid, ap_password) => {
                let connect_state = self.captive_portal.get_connect_state();
                match connect_state {
                    ConnectState::Connecting(ssid, password) => {
                        log::info!(
                            "received provisioning credentials for ssid='{ssid}', password_len={} - switching to station mode",
                            password.len()
                        );

                        wifi::stop_ap()?;

                        match wifi::connect(&ssid, &password) {
                            Ok(()) => {
                                self.captive_portal.mark_connected()?;
                                self.captive_portal.stop()?;
                                self.state = ProvisioningState::Finished(ssid, password);
                                return Ok(NetworkPhase::Connected);
                            }
                            Err(error) => {
                                let error_text = error.to_string();
                                log::error!(
                                    "failed to connect to provisioned network '{ssid}': {error_text}; restarting provisioning AP"
                                );
                                wifi::start_ap(&ap_ssid, &ap_password)?;
                                self.captive_portal.reset_connect_state()?;
                                self.state = ProvisioningState::Started(ap_ssid, ap_password);
                            }
                        }
                    }
                    _ => {
                        self.state = ProvisioningState::Started(ap_ssid, ap_password);
                        // The captive portal is running in a background task.
                        // We will check the status on the next tick.
                    }
                }

                Ok(NetworkPhase::Provisioning)
            }
            ProvisioningState::Finished(ssid, password) => {
                self.state = ProvisioningState::Finished(ssid, password);
                Ok(NetworkPhase::Connected)
            }
        }
    }

    fn get_provisioning_ap_details(&self) -> Option<(String, String)> {
        match &self.state {
            ProvisioningState::Started(ssid, password) => Some((ssid.clone(), password.clone())),
            _ => None,
        }
    }
}

pub fn create_provisioning_manager() -> Box<dyn ProvisioningManager> {
    #[cfg(target_os = "espidf")]
    {
        return Box::new(EspProvisioningManager::default());
    }

    #[cfg(not(target_os = "espidf"))]
    {
        Box::new(HostProvisioningManager)
    }
}
