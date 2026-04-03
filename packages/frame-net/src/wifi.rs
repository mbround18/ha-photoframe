use anyhow::Result;

#[cfg(target_os = "espidf")]
use anyhow::anyhow;
#[cfg(target_os = "espidf")]
use once_cell::sync::OnceCell;
#[cfg(target_os = "espidf")]
use std::sync::Mutex;
#[cfg(target_os = "espidf")]
use std::time::Duration;

#[cfg(target_os = "espidf")]
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::modem::Modem,
    ipv4,
    wifi::{
        AccessPointConfiguration, AuthMethod, BlockingWifi, ClientConfiguration, Configuration,
        EspWifi,
    },
};

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct WifiAp {
    pub ssid: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WifiConnectionState {
    Disconnected,
    Connecting,
    Connected,
}

#[cfg(target_os = "espidf")]
const ALLOW_HOSTED_OPEN_AP_FALLBACK: bool = true;
#[cfg(target_os = "espidf")]
const STA_CONNECT_RETRIES: usize = 5;

#[cfg(target_os = "espidf")]
static WIFI_MANAGER: OnceCell<Mutex<WifiManager>> = OnceCell::new();

#[cfg(target_os = "espidf")]
struct WifiManager {
    #[cfg(target_os = "espidf")]
    wifi: BlockingWifi<EspWifi<'static>>,
}

#[cfg(target_os = "espidf")]
fn ap_credentials_from_config(config: &Configuration) -> Option<(String, String)> {
    match config {
        Configuration::AccessPoint(ap) => Some((ap.ssid.to_string(), ap.password.to_string())),
        Configuration::Mixed(_, ap) => Some((ap.ssid.to_string(), ap.password.to_string())),
        _ => None,
    }
}

#[cfg(target_os = "espidf")]
fn is_known_hosted_open_ap(ssid: &str, password: &str, auth_open: bool) -> bool {
    auth_open
        && password.is_empty()
        && ssid.starts_with("ESP_")
        && ssid.len() == 10
        && ssid[4..].chars().all(|ch| ch.is_ascii_hexdigit())
}

pub fn init_wifi_manager(
    #[cfg(target_os = "espidf")] modem: Modem<'static>,
    #[cfg(target_os = "espidf")] sys_loop: EspSystemEventLoop,
) -> Result<()> {
    #[cfg(target_os = "espidf")]
    {
        let wifi = BlockingWifi::wrap(EspWifi::new(modem, sys_loop.clone(), None)?, sys_loop)?;
        let wifi_manager = WifiManager { wifi };
        if WIFI_MANAGER.set(Mutex::new(wifi_manager)).is_err() {
            return Err(anyhow!("failed to set WIFI_MANAGER"));
        }
    }
    Ok(())
}

#[cfg(target_os = "espidf")]
fn with_wifi_manager<T>(f: impl FnOnce(&mut WifiManager) -> Result<T>) -> Result<T> {
    let manager = WIFI_MANAGER
        .get()
        .ok_or_else(|| anyhow!("WifiManager is not initialized"))?;
    let mut guard = manager
        .lock()
        .map_err(|_| anyhow!("WifiManager mutex poisoned"))?;
    f(&mut guard)
}

#[cfg(target_os = "espidf")]
fn client_config_from(ssid: &str, password: &str) -> Result<ClientConfiguration> {
    Ok(ClientConfiguration {
        ssid: ssid
            .try_into()
            .map_err(|_| anyhow!("ssid too long for client configuration"))?,
        password: password
            .try_into()
            .map_err(|_| anyhow!("password too long for client configuration"))?,
        auth_method: if password.is_empty() {
            AuthMethod::None
        } else {
            AuthMethod::WPA2Personal
        },
        ..Default::default()
    })
}

#[cfg(target_os = "espidf")]
fn ap_config_from(ssid: &str, password: &str) -> Result<AccessPointConfiguration> {
    Ok(AccessPointConfiguration {
        ssid: ssid
            .try_into()
            .map_err(|_| anyhow!("ssid too long for AP configuration"))?,
        password: password
            .try_into()
            .map_err(|_| anyhow!("password too long for AP configuration"))?,
        auth_method: if password.is_empty() {
            AuthMethod::None
        } else {
            AuthMethod::WPA2Personal
        },
        ..Default::default()
    })
}

#[cfg(not(target_os = "espidf"))]
pub fn scan() -> anyhow::Result<Vec<WifiAp>> {
    Ok(vec![
        WifiAp {
            ssid: "Mock WiFi 1".to_string(),
        },
        WifiAp {
            ssid: "Mock WiFi 2".to_string(),
        },
        WifiAp {
            ssid: "Mock WiFi 3".to_string(),
        },
    ])
}

#[cfg(target_os = "espidf")]
pub fn scan() -> anyhow::Result<Vec<WifiAp>> {
    with_wifi_manager(|wifi_manager| {
        let ap_infos = wifi_manager.wifi.scan()?;
        let aps = ap_infos
            .into_iter()
            .map(|ap| WifiAp {
                ssid: ap.ssid.to_string(),
            })
            .collect();
        Ok(aps)
    })
}

#[cfg(not(target_os = "espidf"))]
pub fn connect(ssid: &str, password: &str) -> Result<()> {
    log::info!("(mock) connecting to wifi network: {ssid} with password: {password}");
    Ok(())
}

#[cfg(target_os = "espidf")]
pub fn connect(ssid: &str, password: &str) -> Result<()> {
    with_wifi_manager(|wifi_manager| {
        let mut last_error = None;

        for attempt in 1..=STA_CONNECT_RETRIES {
            if let Err(err) = wifi_manager.wifi.stop() {
                log::debug!("STA pre-stop ignored on attempt {attempt}: {err}");
            }

            let client_config = client_config_from(ssid, password)?;
            wifi_manager
                .wifi
                .set_configuration(&Configuration::Client(client_config))?;

            if let Ok(Configuration::Client(applied)) = wifi_manager.wifi.get_configuration() {
                log::info!(
                    "STA config applied on attempt {attempt}: ssid='{}', password_len={}, auth={:?}",
                    applied.ssid,
                    applied.password.len(),
                    applied.auth_method
                );
            }

            wifi_manager.wifi.start()?;
            log::info!(
                "Wi-Fi station started for ssid='{ssid}' on attempt {attempt}/{STA_CONNECT_RETRIES}"
            );

            match wifi_manager.wifi.connect() {
                Ok(()) => log::info!("Wi-Fi association requested on attempt {attempt}"),
                Err(error) => {
                    let error_text = error.to_string();
                    log::warn!(
                        "Wi-Fi connect command failed on attempt {attempt}/{STA_CONNECT_RETRIES} for ssid='{ssid}': {error_text}"
                    );
                    last_error = Some(error_text);
                    std::thread::sleep(Duration::from_secs(1));
                    continue;
                }
            }

            match wifi_manager.wifi.wait_netif_up() {
                Ok(()) => {
                    log::info!("Wi-Fi netif up for ssid='{ssid}' on attempt {attempt}");
                    return Ok(());
                }
                Err(error) => {
                    let error_text = error.to_string();
                    log::warn!(
                        "Wi-Fi failed to get an IP on attempt {attempt}/{STA_CONNECT_RETRIES} for ssid='{ssid}': {error_text}"
                    );
                    last_error = Some(error_text);
                    std::thread::sleep(Duration::from_secs(1));
                }
            }
        }

        Err(anyhow!(
            "failed to connect to ssid '{ssid}' after {STA_CONNECT_RETRIES} attempts: {}",
            last_error.unwrap_or_else(|| "unknown station error".to_string())
        ))
    })
}

#[cfg(not(target_os = "espidf"))]
pub fn start_ap(ssid: &str, password: &str) -> Result<()> {
    log::info!("(mock) starting ap: {ssid} with password: {password}");
    Ok(())
}

#[cfg(target_os = "espidf")]
pub fn start_ap(ssid: &str, password: &str) -> Result<()> {
    with_wifi_manager(|wifi_manager| {
        let mut last_observation = String::from("no AP configuration observed");
        let mut last_effective_ap: Option<(String, String, bool)> = None;

        log::info!(
            "starting AP request: ssid='{ssid}', password_len={}, protected={}",
            password.len(),
            !password.is_empty()
        );

        for attempt in 1..=5 {
            if let Err(err) = wifi_manager.wifi.stop() {
                log::debug!("AP pre-stop ignored (attempt {attempt}): {err}");
            }

            let ap_config = ap_config_from(ssid, password)?;
            wifi_manager
                .wifi
                .set_configuration(&Configuration::AccessPoint(ap_config))?;

            wifi_manager.wifi.start()?;
            wifi_manager.wifi.wait_netif_up()?;

            // Hosted Wi-Fi may overwrite AP config during startup; apply once more after netif is up.
            let ap_config_post_start = ap_config_from(ssid, password)?;
            wifi_manager
                .wifi
                .set_configuration(&Configuration::AccessPoint(ap_config_post_start))?;
            std::thread::sleep(Duration::from_millis(200));

            let mut matched = false;
            if let Ok(config) = wifi_manager.wifi.get_configuration() {
                if let Some((effective_ssid, effective_password)) =
                    ap_credentials_from_config(&config)
                {
                    let expected_auth_open = password.is_empty();
                    let effective_auth_open = match &config {
                        Configuration::AccessPoint(ap) => ap.auth_method == AuthMethod::None,
                        Configuration::Mixed(_, ap) => ap.auth_method == AuthMethod::None,
                        _ => false,
                    };

                    matched = effective_ssid == ssid
                        && effective_password == password
                        && expected_auth_open == effective_auth_open;

                    if matched {
                        log::info!(
                            "AP started with requested credentials: ssid='{effective_ssid}', password_len={}, protected={}",
                            effective_password.len(),
                            !effective_auth_open
                        );
                        return Ok(());
                    }

                    if ALLOW_HOSTED_OPEN_AP_FALLBACK
                        && is_known_hosted_open_ap(
                            &effective_ssid,
                            &effective_password,
                            effective_auth_open,
                        )
                    {
                        log::warn!(
                            "using hosted fallback AP ssid='{effective_ssid}' because the coprocessor did not accept the requested provisioning SSID '{ssid}'"
                        );
                        return Ok(());
                    }

                    log::warn!(
                        "AP config mismatch on attempt {attempt}: requested ssid='{ssid}', password_len={}, protected={} but active ssid='{effective_ssid}', password_len={}, protected={}",
                        password.len(),
                        !expected_auth_open,
                        effective_password.len(),
                        !effective_auth_open
                    );

                    last_observation = format!(
                        "attempt {attempt}: active ssid='{effective_ssid}', password_len={}, protected={}",
                        effective_password.len(),
                        !effective_auth_open
                    );
                    last_effective_ap = Some((
                        effective_ssid.clone(),
                        effective_password.clone(),
                        effective_auth_open,
                    ));
                }
            } else {
                last_observation =
                    format!("attempt {attempt}: unable to read active AP configuration");
            }

            if !matched {
                std::thread::sleep(Duration::from_millis(500));
            }
        }

        if ALLOW_HOSTED_OPEN_AP_FALLBACK {
            if let Some((effective_ssid, effective_password, effective_auth_open)) =
                last_effective_ap.as_ref()
            {
                if is_known_hosted_open_ap(effective_ssid, effective_password, *effective_auth_open)
                {
                    log::error!(
                        "SECURITY WARNING: booting with hosted fallback AP ssid='{effective_ssid}' as an open network because requested provisioning AP credentials could not be enforced"
                    );
                    return Ok(());
                }
            }
        }

        Err(anyhow!(
            "failed to apply AP credentials after retries; refusing to continue with unknown AP security; {last_observation}"
        ))
    })
}

#[cfg(not(target_os = "espidf"))]
pub fn stop_ap() -> Result<()> {
    log::info!("(mock) stopping ap");
    Ok(())
}

#[cfg(not(target_os = "espidf"))]
pub fn current_ap_credentials() -> Result<Option<(String, String)>> {
    Ok(None)
}

#[cfg(target_os = "espidf")]
pub fn current_ap_credentials() -> Result<Option<(String, String)>> {
    with_wifi_manager(|wifi_manager| {
        let config = wifi_manager.wifi.get_configuration()?;
        Ok(ap_credentials_from_config(&config))
    })
}

#[cfg(target_os = "espidf")]
pub fn stop_ap() -> Result<()> {
    with_wifi_manager(|wifi_manager| {
        wifi_manager.wifi.stop()?;
        log::info!("AP stopped");
        Ok(())
    })
}

#[cfg(not(target_os = "espidf"))]
pub fn current_sta_ip() -> Result<Option<String>> {
    Ok(None)
}

#[cfg(target_os = "espidf")]
pub fn current_sta_ip() -> Result<Option<String>> {
    with_wifi_manager(|wifi_manager| {
        let ip_info = wifi_manager.wifi.wifi().sta_netif().get_ip_info()?;
        if ip_info.ip == ipv4::Ipv4Addr::new(0, 0, 0, 0) {
            Ok(None)
        } else {
            Ok(Some(ip_info.ip.to_string()))
        }
    })
}
