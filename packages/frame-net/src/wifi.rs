use anyhow::{anyhow, Result};
use once_cell::sync::OnceCell;

#[cfg(target_os = "espidf")]
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::modem::Modem,
    wifi::{AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WifiConnectionState {
    Disconnected,
    Connecting,
    Connected,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WifiAp {
    pub ssid: String,
}

static WIFI_MANAGER: OnceCell<WifiManager> = OnceCell::new();

struct WifiManager {
    #[cfg(target_os = "espidf")]
    wifi: BlockingWifi<EspWifi<'static>>,
}

pub fn init_wifi_manager(
    #[cfg(target_os = "espidf")] modem: Modem,
    #[cfg(target_os = "espidf")] sys_loop: EspSystemEventLoop,
) -> Result<()> {
    #[cfg(target_os = "espidf")]
    {
        let wifi = BlockingWifi::wrap(EspWifi::new(modem, sys_loop.clone(), None)?, sys_loop)?;
        let wifi_manager = WifiManager { wifi };
        if WIFI_MANAGER.set(wifi_manager).is_err() {
            return Err(anyhow!("failed to set WIFI_MANAGER"));
        }
    }
    Ok(())
}

fn get_wifi_manager() -> Result<&'static WifiManager> {
    WIFI_MANAGER
        .get()
        .ok_or_else(|| anyhow!("WifiManager is not initialized"))
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
    let mut wifi_manager = get_wifi_manager()?;
    let ap_infos = wifi_manager.wifi.scan()?;
    let aaps = ap_infos
        .into_iter()
        .map(|ap| WifiAp {
            ssid: ap.ssid.to_string(),
        })
        .collect();
    Ok(aaps)
}

#[cfg(not(target_os = "espidf"))]
pub fn connect(ssid: &str, password: &str) -> Result<()> {
    log::info!("(mock) connecting to wifi network: {ssid} with password: {password}");
    Ok(())
}

#[cfg(target_os = "espidf")]
pub fn connect(ssid: &str, password: &str) -> Result<()> {
    let wifi_manager = get_wifi_manager()?;

    let client_config = ClientConfiguration {
        ssid: ssid.into(),
        password: password.into(),
        auth_method: AuthMethod::WPA2Personal,
        ..Default::default()
    };
    wifi_manager
        .wifi
        .set_configuration(&Configuration::Client(client_config))?;

    wifi_manager.wifi.start()?;
    log::info!("Wi-Fi started");

    wifi_manager.wifi.connect()?;
    log::info!("Wi-Fi connected");

    wifi_manager.wifi.wait_netif_up()?;
    log::info!("Wi-Fi netif up");

    Ok(())
}
