use anyhow::{anyhow, Result};
use serde::Serialize;
use std::sync::{Arc, Mutex};

#[cfg(target_os = "espidf")]
use esp_idf_svc::http::server::{Configuration as HttpConfiguration, EspHttpServer, Method};
#[cfg(target_os = "espidf")]
use esp_idf_svc::io::Write;
#[cfg(target_os = "espidf")]
use std::collections::HashMap;
#[cfg(target_os = "espidf")]
use std::net::{Ipv4Addr, UdpSocket};

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ConnectState {
    NotConnected,
    Connecting(String, String),
    Connected,
}

#[cfg(target_os = "espidf")]
const CAPTIVE_PORTAL_IP: Ipv4Addr = Ipv4Addr::new(192, 168, 71, 1);

#[derive(Clone, PartialEq, Eq, Debug, Serialize)]
pub struct WifiNetwork {
    pub ssid: String,
}

#[cfg(target_os = "espidf")]
#[derive(Debug, Serialize)]
struct WifiScanResponse {
    networks: Vec<WifiNetwork>,
    error: Option<String>,
}

type ScanHandler = Arc<dyn Fn() -> Result<Vec<WifiNetwork>> + Send + Sync>;

struct PortalState {
    connect_state: ConnectState,
    scan_handler: Option<ScanHandler>,
}

pub struct CaptivePortal {
    state: Arc<Mutex<PortalState>>,
    #[cfg(target_os = "espidf")]
    server: Option<EspHttpServer<'static>>,
}

pub fn create_captive_portal() -> Result<CaptivePortal> {
    Ok(CaptivePortal {
        state: Arc::new(Mutex::new(PortalState {
            connect_state: ConnectState::NotConnected,
            scan_handler: None,
        })),
        #[cfg(target_os = "espidf")]
        server: None,
    })
}

impl CaptivePortal {
    pub fn set_scan_handler<F>(&mut self, handler: F) -> Result<()>
    where
        F: Fn() -> Result<Vec<WifiNetwork>> + Send + Sync + 'static,
    {
        let mut guard = self
            .state
            .lock()
            .map_err(|_| anyhow!("captive portal state lock poisoned"))?;
        guard.scan_handler = Some(Arc::new(handler));
        Ok(())
    }

    pub async fn run(&self) -> Result<()> {
        Ok(())
    }

    pub fn mark_connected(&mut self) -> Result<()> {
        let mut guard = self
            .state
            .lock()
            .map_err(|_| anyhow!("captive portal state lock poisoned"))?;
        guard.connect_state = ConnectState::Connected;
        Ok(())
    }

    pub fn reset_connect_state(&mut self) -> Result<()> {
        let mut guard = self
            .state
            .lock()
            .map_err(|_| anyhow!("captive portal state lock poisoned"))?;
        guard.connect_state = ConnectState::NotConnected;
        Ok(())
    }

    #[cfg(not(target_os = "espidf"))]
    pub fn stop(&mut self) -> Result<()> {
        Ok(())
    }

    #[cfg(target_os = "espidf")]
    pub fn stop(&mut self) -> Result<()> {
        self.server.take();
        Ok(())
    }

    #[cfg(not(target_os = "espidf"))]
    pub fn start(&mut self) -> Result<()> {
        Ok(())
    }

    #[cfg(target_os = "espidf")]
    pub fn start(&mut self) -> Result<()> {
        if self.server.is_some() {
            return Ok(());
        }

        static INDEX_HTML: &str = include_str!("index.html");
        let portal_url = format!("http://{CAPTIVE_PORTAL_IP}/");

        let mut server = EspHttpServer::new(&HttpConfiguration::default())?;

        server.fn_handler("/", Method::Get, move |request| -> Result<()> {
            let mut response =
                request.into_response(200, Some("OK"), &[("Content-Type", "text/html")])?;
            response.write_all(INDEX_HTML.as_bytes())?;
            Ok(())
        })?;

        server.fn_handler("/index.html", Method::Get, move |request| -> Result<()> {
            let mut response =
                request.into_response(200, Some("OK"), &[("Content-Type", "text/html")])?;
            response.write_all(INDEX_HTML.as_bytes())?;
            Ok(())
        })?;

        for path in [
            "/generate_204",
            "/gen_204",
            "/hotspot-detect.html",
            "/library/test/success.html",
            "/success.txt",
            "/ncsi.txt",
            "/connecttest.txt",
            "/redirect",
            "/fwlink",
            "/canonical.html",
        ] {
            let location = portal_url.clone();
            server.fn_handler(path, Method::Get, move |request| -> Result<()> {
                let mut response = request.into_response(
                    302,
                    Some("Found"),
                    &[
                        ("Location", location.as_str()),
                        ("Cache-Control", "no-store"),
                    ],
                )?;
                response.write_all(&[])?;
                Ok(())
            })?;
        }

        let scan_state = Arc::clone(&self.state);
        server.fn_handler("/wifi-scan", Method::Get, move |request| -> Result<()> {
            let scan_handler = {
                let guard = scan_state
                    .lock()
                    .map_err(|_| anyhow!("captive portal state lock poisoned"))?;
                guard.scan_handler.clone()
            };

            let response_body = if let Some(scan) = scan_handler {
                match scan() {
                    Ok(networks) => WifiScanResponse {
                        networks,
                        error: None,
                    },
                    Err(error) => {
                        log::warn!("wifi scan unavailable while captive portal is active: {error}");
                        WifiScanResponse {
                            networks: Vec::new(),
                            error: Some(
                                "Wi-Fi scan is unavailable while setup mode is active on this firmware. Enter your network name manually."
                                    .to_string(),
                            ),
                        }
                    }
                }
            } else {
                WifiScanResponse {
                    networks: Vec::new(),
                    error: Some("Wi-Fi scan handler is not configured yet.".to_string()),
                }
            };

            let body = serde_json::to_string(&response_body)?;
            let mut response =
                request.into_response(200, Some("OK"), &[("Content-Type", "application/json")])?;
            response.write_all(body.as_bytes())?;
            Ok(())
        })?;

        let connect_state = Arc::clone(&self.state);
        server.fn_handler("/connect", Method::Post, move |mut request| -> Result<()> {
            let mut body = vec![0_u8; 2048];
            let read = request.read(&mut body)?;
            let form = String::from_utf8_lossy(&body[..read]).to_string();
            let form_data = parse_form_encoded(&form);

            let ssid = form_data
                .get("manual_ssid")
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .or_else(|| {
                    form_data
                        .get("ssid")
                        .map(|value| value.trim())
                        .filter(|value| !value.is_empty())
                        .map(ToOwned::to_owned)
                })
                .unwrap_or_default();
            let password = form_data.get("password").cloned().unwrap_or_default();

            if ssid.is_empty() {
                let mut response = request.into_response(
                    400,
                    Some("Bad Request"),
                    &[("Content-Type", "text/plain")],
                )?;
                response.write_all(b"Missing Wi-Fi network name. Enter the SSID manually or choose one from scan results.")?;
                return Ok(());
            }

            {
                let mut guard = connect_state
                    .lock()
                    .map_err(|_| anyhow!("captive portal state lock poisoned"))?;
                guard.connect_state = ConnectState::Connecting(ssid, password);
            }

            let mut response =
                request.into_response(200, Some("OK"), &[("Content-Type", "text/html")])?;
            response.write_all(
                br#"<!doctype html><html><head><meta charset='utf-8'><meta name='viewport' content='width=device-width,initial-scale=1'><title>Connecting your frame</title><style>:root{color-scheme:dark;--bg:#09111d;--panel:#0d1728;--panel-strong:#10253a;--border:#27415d;--text:#f8fafc;--muted:#b8c6d8;--accent:#7dd3fc;--accent-soft:#14304a}*{box-sizing:border-box}body{margin:0;min-height:100vh;display:grid;place-items:center;padding:24px;font-family:"Avenir Next","Segoe UI Variable","Segoe UI",sans-serif;background:radial-gradient(circle at top left,#14304a 0,transparent 34%),radial-gradient(circle at bottom right,#0f3b38 0,transparent 28%),var(--bg);color:var(--text)}main{width:min(100%,36rem);padding:28px;border-radius:28px;border:1px solid var(--border);background:linear-gradient(180deg,var(--panel-strong),var(--panel));box-shadow:0 24px 60px rgba(0,0,0,.35)}.badge{display:inline-flex;padding:8px 14px;border-radius:999px;background:var(--accent-soft);color:#dbeafe;font-size:.82rem;font-weight:700;letter-spacing:.04em;text-transform:uppercase}.pulse{width:72px;height:72px;border-radius:50%;margin:18px 0;background:radial-gradient(circle,var(--accent) 0,var(--accent) 18%,rgba(125,211,252,.18) 20%,rgba(125,211,252,0) 62%);animation:pulse 1.8s ease-in-out infinite}@keyframes pulse{0%,100%{transform:scale(.96);opacity:.85}50%{transform:scale(1.03);opacity:1}}h1{margin:0 0 12px;font-size:2rem;line-height:1.05}p{margin:0 0 12px;color:var(--muted);line-height:1.6}.note{margin-top:18px;padding:16px 18px;border-radius:20px;border:1px solid rgba(125,211,252,.2);background:rgba(9,17,29,.45)}strong{color:var(--text)}</style></head><body><main><div class='badge'>Switching networks</div><div class='pulse' aria-hidden='true'></div><h1>Your frame is joining Wi-Fi</h1><p>The frame is leaving its temporary setup network and trying the home Wi-Fi you selected.</p><p class='note'>If this page does not close on its own, wait a few seconds and reconnect your phone or laptop to its normal network. Then open the frame again on your local setup address if needed.</p></main></body></html>"#,
            )?;
            Ok(())
        })?;

        start_captive_dns_task()?;
        self.server = Some(server);
        Ok(())
    }

    pub fn get_connect_state(&self) -> ConnectState {
        match self.state.lock() {
            Ok(guard) => guard.connect_state.clone(),
            Err(_) => ConnectState::NotConnected,
        }
    }
}

#[cfg(target_os = "espidf")]
fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(target_os = "espidf")]
fn start_captive_dns_task() -> Result<()> {
    std::thread::Builder::new()
        .name("captive-dns".into())
        .spawn(|| {
            let socket = match UdpSocket::bind(("0.0.0.0", 53)) {
                Ok(socket) => socket,
                Err(error) => {
                    log::error!("failed to bind captive DNS socket: {error}");
                    return;
                }
            };

            let mut buffer = [0_u8; 512];
            loop {
                let (len, peer) = match socket.recv_from(&mut buffer) {
                    Ok(result) => result,
                    Err(error) => {
                        log::warn!("captive DNS recv failed: {error}");
                        continue;
                    }
                };

                if let Some(response) =
                    build_captive_dns_response(&buffer[..len], CAPTIVE_PORTAL_IP)
                {
                    if let Err(error) = socket.send_to(&response, peer) {
                        log::warn!("captive DNS send failed: {error}");
                    }
                }
            }
        })
        .map(|_| ())
        .map_err(|error| anyhow!("failed to spawn captive DNS task: {error}"))
}

#[cfg(target_os = "espidf")]
fn build_captive_dns_response(query: &[u8], portal_ip: Ipv4Addr) -> Option<Vec<u8>> {
    if query.len() < 12 {
        return None;
    }

    let question_count = u16::from_be_bytes([query[4], query[5]]);
    if question_count == 0 {
        return None;
    }

    let mut cursor = 12;
    while cursor < query.len() {
        let label_len = *query.get(cursor)? as usize;
        cursor += 1;
        if label_len == 0 {
            break;
        }
        cursor = cursor.checked_add(label_len)?;
    }

    let question_end = cursor.checked_add(4)?;
    if question_end > query.len() {
        return None;
    }

    let qtype = u16::from_be_bytes([query[cursor], query[cursor + 1]]);
    let qclass = u16::from_be_bytes([query[cursor + 2], query[cursor + 3]]);

    let answer_count = if qtype == 1 && qclass == 1 {
        1_u16
    } else {
        0_u16
    };

    let mut response = Vec::with_capacity(question_end + 16);
    response.extend_from_slice(&query[0..2]);
    response.extend_from_slice(&[0x81, 0x80]);
    response.extend_from_slice(&query[4..6]);
    response.extend_from_slice(&answer_count.to_be_bytes());
    response.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
    response.extend_from_slice(&query[12..question_end]);

    if answer_count == 1 {
        response.extend_from_slice(&[0xC0, 0x0C]);
        response.extend_from_slice(&1_u16.to_be_bytes());
        response.extend_from_slice(&1_u16.to_be_bytes());
        response.extend_from_slice(&60_u32.to_be_bytes());
        response.extend_from_slice(&4_u16.to_be_bytes());
        response.extend_from_slice(&portal_ip.octets());
    }

    Some(response)
}

#[cfg(target_os = "espidf")]
fn parse_form_encoded(input: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for pair in input.split('&') {
        if pair.is_empty() {
            continue;
        }

        let mut parts = pair.splitn(2, '=');
        let raw_key = parts.next().unwrap_or_default();
        let raw_value = parts.next().unwrap_or_default();
        let key = percent_decode(raw_key);
        let value = percent_decode(raw_value);
        out.insert(key, value);
    }
    out
}

#[cfg(target_os = "espidf")]
fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = String::with_capacity(value.len());
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let h1 = bytes[i + 1] as char;
                let h2 = bytes[i + 2] as char;
                if let (Some(a), Some(b)) = (h1.to_digit(16), h2.to_digit(16)) {
                    out.push((a * 16 + b) as u8 as char);
                    i += 3;
                } else {
                    out.push('%');
                    i += 1;
                }
            }
            other => {
                out.push(other as char);
                i += 1;
            }
        }
    }

    out
}
