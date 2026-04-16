use anyhow::{anyhow, Context, Result};
use frame_api::oauth::DeviceAccessToken;
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

#[derive(Clone, Debug, Default)]
struct LocalSetupSession {
    setup: LocalSetupState,
    pairing_verified: bool,
    pairing_error: Option<String>,
    browser_auth: BrowserAuthSession,
}

#[cfg_attr(not(target_os = "espidf"), allow(dead_code))]
#[derive(Clone, Debug, Default)]
struct BrowserAuthSession {
    authorization_url: Option<String>,
    redirect_uri: Option<String>,
    state: Option<String>,
    pkce_verifier: Option<String>,
    access_token: Option<DeviceAccessToken>,
    error: Option<String>,
}

#[cfg(target_os = "espidf")]
#[derive(Clone, Debug, Serialize)]
struct LocalSetupStatusResponse {
    status: String,
    detail: String,
    owner_email: Option<String>,
    pairing_required: bool,
    pairing_verified: bool,
    pairing_error: Option<String>,
    local_setup_url: Option<String>,
    local_setup_ip_url: Option<String>,
    browser_auth_url: Option<String>,
    browser_auth_error: Option<String>,
    browser_auth_pending: bool,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct LocalSetupState {
    pub status: String,
    pub detail: String,
    pub owner_email: Option<String>,
    pub pairing_code: Option<String>,
    pub local_setup_url: Option<String>,
    pub local_setup_ip_url: Option<String>,
    pub auth_verification_uri: Option<String>,
    pub auth_user_code: Option<String>,
    pub device_id: Option<String>,
    pub device_name: Option<String>,
}

pub struct CaptivePortal {
    state: Arc<Mutex<PortalState>>,
    #[cfg(target_os = "espidf")]
    server: Option<EspHttpServer<'static>>,
}

pub struct LocalSetupServer {
    state: Arc<Mutex<LocalSetupSession>>,
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

pub fn create_local_setup_server() -> Result<LocalSetupServer> {
    Ok(LocalSetupServer {
        state: Arc::new(Mutex::new(LocalSetupSession::default())),
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

impl LocalSetupServer {
    pub fn update_state(&mut self, next_state: LocalSetupState) -> Result<()> {
        let mut guard = self
            .state
            .lock()
            .map_err(|_| anyhow!("local setup state lock poisoned"))?;
        if guard.setup.pairing_code != next_state.pairing_code {
            guard.pairing_verified = false;
            guard.pairing_error = None;
        }
        guard.setup = next_state;
        Ok(())
    }

    pub fn pairing_verified(&self) -> Result<bool> {
        let guard = self
            .state
            .lock()
            .map_err(|_| anyhow!("local setup state lock poisoned"))?;
        Ok(guard.pairing_verified)
    }

    pub fn take_browser_access_token(&mut self) -> Result<Option<DeviceAccessToken>> {
        let mut guard = self
            .state
            .lock()
            .map_err(|_| anyhow!("local setup state lock poisoned"))?;
        Ok(guard.browser_auth.access_token.take())
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

        let mut server = EspHttpServer::new(&HttpConfiguration::default())?;
        let state = Arc::clone(&self.state);
        server.fn_handler("/", Method::Get, move |request| -> Result<()> {
            let request_uri = request.uri().to_string();
            let snapshot = {
                let mut guard = state
                    .lock()
                    .map_err(|_| anyhow!("local setup state lock poisoned"))?;
                apply_link_code_from_uri(&mut guard, &request_uri);
                guard.clone()
            };
            let body = render_local_setup_html(&snapshot);
            let mut response =
                request.into_response(200, Some("OK"), &[("Content-Type", "text/html")])?;
            response.write_all(body.as_bytes())?;
            Ok(())
        })?;

        let state = Arc::clone(&self.state);
        server.fn_handler("/pair", Method::Post, move |mut request| -> Result<()> {
            let mut body = vec![0_u8; 1024];
            let read = request.read(&mut body)?;
            let form = String::from_utf8_lossy(&body[..read]).to_string();
            let form_data = parse_form_encoded(&form);
            let submitted_code = form_data
                .get("pairing_code")
                .map(|value| value.trim())
                .unwrap_or_default();

            let snapshot = {
                let mut guard = state
                    .lock()
                    .map_err(|_| anyhow!("local setup state lock poisoned"))?;
                validate_pairing_code(
                    &mut guard,
                    submitted_code,
                    "That pairing code does not match the one currently shown on the frame.",
                );

                guard.clone()
            };

            let body = render_local_setup_html(&snapshot);
            let mut response =
                request.into_response(200, Some("OK"), &[("Content-Type", "text/html")])?;
            response.write_all(body.as_bytes())?;
            Ok(())
        })?;

        let state = Arc::clone(&self.state);
        server.fn_handler("/status", Method::Get, move |request| -> Result<()> {
            let snapshot = state
                .lock()
                .map_err(|_| anyhow!("local setup state lock poisoned"))?
                .clone();
            let body = serde_json::to_string(&local_setup_status_response(&snapshot))?;
            let mut response =
                request.into_response(200, Some("OK"), &[("Content-Type", "application/json")])?;
            response.write_all(body.as_bytes())?;
            Ok(())
        })?;

        let state = Arc::clone(&self.state);
        server.fn_handler("/oauth/start", Method::Get, move |request| -> Result<()> {
            let authorization_url = {
                let mut guard = state
                    .lock()
                    .map_err(|_| anyhow!("local setup state lock poisoned"))?;
                prepare_browser_authorization(&mut guard)?
            };

            let mut response = request.into_response(
                302,
                Some("Found"),
                &[
                    ("Location", authorization_url.as_str()),
                    ("Cache-Control", "no-store"),
                ],
            )?;
            response.write_all(&[])?;
            Ok(())
        })?;

        let state = Arc::clone(&self.state);
        server.fn_handler(
            "/oauth/callback",
            Method::Get,
            move |request| -> Result<()> {
                let request_uri = request.uri().to_string();
                let body = {
                    let mut guard = state
                        .lock()
                        .map_err(|_| anyhow!("local setup state lock poisoned"))?;
                    finish_browser_authorization(&mut guard, &request_uri);
                    render_browser_callback_html(&guard)
                };

                let mut response =
                    request.into_response(200, Some("OK"), &[("Content-Type", "text/html")])?;
                response.write_all(body.as_bytes())?;
                Ok(())
            },
        )?;

        self.server = Some(server);
        Ok(())
    }
}

#[cfg(target_os = "espidf")]
fn local_setup_status_response(session: &LocalSetupSession) -> LocalSetupStatusResponse {
    LocalSetupStatusResponse {
        status: session.setup.status.clone(),
        detail: session.setup.detail.clone(),
        owner_email: session.setup.owner_email.clone(),
        pairing_required: session.setup.pairing_code.is_some(),
        pairing_verified: session.pairing_verified,
        pairing_error: session.pairing_error.clone(),
        local_setup_url: session.setup.local_setup_url.clone(),
        local_setup_ip_url: session.setup.local_setup_ip_url.clone(),
        browser_auth_url: session
            .pairing_verified
            .then_some("/oauth/start".to_string()),
        browser_auth_error: session.browser_auth.error.clone(),
        browser_auth_pending: session.browser_auth.authorization_url.is_some()
            && session.browser_auth.access_token.is_none(),
    }
}

#[cfg(target_os = "espidf")]
fn render_local_setup_html(session: &LocalSetupSession) -> String {
    let local_url = session
        .setup
        .local_setup_url
        .as_deref()
        .unwrap_or("Unavailable");
    let ip_url = session
        .setup
        .local_setup_ip_url
        .as_deref()
        .unwrap_or("Unavailable");
    let sign_in_url = session
        .browser_auth
        .authorization_url
        .as_deref()
        .unwrap_or("/oauth/start");
    let owner_ready = session
        .setup
        .owner_email
        .as_deref()
        .filter(|email| !email.is_empty())
        .is_some();
    let badge = if owner_ready {
        "All set"
    } else if session.pairing_verified {
        "Google Photos"
    } else {
        "Local setup"
    };
    let pairing_state_html = render_pairing_state_html(session);
    let top_action_html = render_top_action_html(session, sign_in_url);
    let google_sign_in_html = render_google_sign_in_html(session, sign_in_url);
    let album_selection_html = render_album_selection_html(session);
    let owner_html = session
        .setup
        .owner_email
        .as_deref()
        .filter(|email| !email.is_empty())
        .map(|email| {
            format!(
                "<section class='tile'><h2>Owner</h2><p class='tile-copy'>Signed in as <strong>{}</strong>.</p></section>",
                escape_html(email)
            )
        })
        .unwrap_or_default();
    let owner_display = if owner_html.is_empty() {
        "none"
    } else {
        "block"
    };
    let script = render_local_setup_poll_script();

    format!(
                "<!doctype html><html><head><meta charset='utf-8'><meta name='viewport' content='width=device-width,initial-scale=1'><title>Photo Frame Local Setup</title><style>:root{{color-scheme:dark;--bg:#09111d;--panel:#0d1728;--panel-strong:#10253a;--panel-soft:#10192a;--border:#27415d;--tile-border:#243247;--text:#f8fafc;--muted:#b8c6d8;--subtle:#9fb2c7;--accent:#7dd3fc;--accent-soft:#14304a;--good:#34d399;--danger:#fca5a5;--warn:#fbbf24}}*{{box-sizing:border-box}}body{{margin:0;padding:20px;font-family:'Avenir Next','Segoe UI Variable','Segoe UI',sans-serif;background:radial-gradient(circle at top left,#14304a 0,transparent 28%),radial-gradient(circle at bottom right,#0f3b38 0,transparent 24%),var(--bg);color:var(--text)}}main{{width:min(100%,72rem);margin:0 auto}}.hero{{padding:28px;border-radius:30px;border:1px solid var(--border);background:linear-gradient(180deg,var(--panel-strong),var(--panel));box-shadow:0 24px 60px rgba(0,0,0,.32)}}.badge{{display:inline-flex;padding:8px 14px;border-radius:999px;background:var(--accent-soft);color:#dbeafe;font-size:.82rem;font-weight:700;letter-spacing:.04em;text-transform:uppercase}}h1{{margin:16px 0 10px;font-size:clamp(2rem,6vw,3.25rem);line-height:1.02}}.hero p{{margin:0;color:var(--muted);max-width:56rem;line-height:1.6}}.hero-actions{{margin-top:18px;display:grid;gap:12px}}.hero-callout{{padding:18px 20px;border-radius:22px;border:1px solid rgba(125,211,252,.22);background:rgba(9,17,29,.42)}}.hero-callout.hidden{{display:none}}.hero-callout-title{{margin:0 0 8px;color:#dbeafe;font-size:1rem;font-weight:800}}.hero-callout-body{{margin:0;color:var(--muted);line-height:1.6}}.hero-link{{display:inline-flex;align-items:center;justify-content:center;padding:14px 18px;border-radius:14px;background:var(--accent);color:#0b1220;font-weight:800;text-decoration:none}}.grid{{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:18px;margin-top:20px}}.tile{{padding:22px;border-radius:24px;border:1px solid var(--tile-border);background:rgba(13,23,40,.92)}}.tile-wide{{grid-column:1 / -1}}.hidden{{display:none}}.album-shell{{display:grid;gap:14px}}.album-shell-card{{padding:18px;border-radius:18px;border:1px solid rgba(251,191,36,.2);background:rgba(9,17,29,.45)}}.album-shell-card strong{{display:block;margin-bottom:6px;color:#fde68a}}h2{{margin:0 0 10px;color:var(--accent);font-size:.92rem;font-weight:700;letter-spacing:.04em;text-transform:uppercase}}.tile-copy{{margin:0;color:var(--muted);line-height:1.6}}.value{{margin:0 0 8px;font-size:1.5rem;font-weight:700;color:var(--text);line-height:1.2}}code{{font-family:'IBM Plex Mono','SFMono-Regular',monospace;background:#020617;padding:3px 7px;border-radius:8px;color:#e2e8f0}}.pair-form{{margin-top:14px;display:flex;flex-wrap:wrap;gap:12px;align-items:end}}label{{display:block;color:#dbe4f0;font-weight:700}}input{{display:block;width:min(100%,18rem);padding:14px 16px;border-radius:14px;border:1px solid #334155;background:#020617;color:#f8fafc;font-size:1rem}}button{{padding:14px 18px;border-radius:14px;border:0;background:var(--accent);color:#0b1220;font-weight:800;cursor:pointer}}.code{{margin:12px 0 8px;font-size:clamp(2rem,7vw,3rem);font-weight:800;letter-spacing:.08em;color:var(--accent)}}.error{{margin:12px 0 0;color:var(--danger);line-height:1.5}}.success{{color:var(--good)}}@media (max-width: 760px){{body{{padding:14px}}.hero{{padding:22px}}.grid{{grid-template-columns:1fr}}.pair-form{{flex-direction:column;align-items:stretch}}input{{width:100%}}button{{width:100%}}.hero-link{{width:100%}}}}</style></head><body><main><section class='hero'><div class='badge' id='badge'>{badge}</div><h1 id='status'>{status}</h1><p id='detail'>{detail}</p><div class='hero-actions'>{top_action_html}</div></section><div class='grid'><section class='tile'><h2>Pass code</h2><div id='pairing-state'>{pairing_state_html}</div></section><section class='tile'><h2>Browser address</h2><p class='value'><code id='local-url'>{local_url}</code></p><p class='tile-copy'>Fallback IP: <code id='ip-url'>{ip_url}</code></p></section><section class='tile' id='owner-tile' style='display:{owner_display}'><h2>Owner</h2><p class='tile-copy'>Signed in as <strong id='owner-email'>{owner_email}</strong>.</p></section><section class='tile'><h2>State summary</h2><p class='value' id='summary-status'>{status}</p><p class='tile-copy' id='summary-detail'>{detail}</p></section>{google_sign_in_html}{album_selection_html}</div></main>{script}</body></html>",
        badge = badge,
        status = escape_html(&session.setup.status),
        detail = escape_html(&session.setup.detail),
                top_action_html = top_action_html,
        pairing_state_html = pairing_state_html,
        local_url = escape_html(local_url),
        ip_url = escape_html(ip_url),
                owner_display = owner_display,
                owner_email = escape_html(session.setup.owner_email.as_deref().unwrap_or("")),
        google_sign_in_html = google_sign_in_html,
                album_selection_html = album_selection_html,
                script = script,
    )
}

#[cfg(target_os = "espidf")]
fn render_pairing_state_html(session: &LocalSetupSession) -> String {
    if session.pairing_verified {
        return "<p class='tile-copy'>This browser is verified against the pass code currently shown on the frame.</p>".to_string();
    }

    let error_display = if session.pairing_error.is_some() {
        "block"
    } else {
        "none"
    };
    let error_text = escape_html(session.pairing_error.as_deref().unwrap_or(""));

    format!(
                "<p class='tile-copy'>Enter the pass code shown on the frame. That code never appears on this webpage, which keeps nearby setup private.</p><p class='error' id='pairing-error' style='display:{error_display}'>{error_text}</p><form class='pair-form' method='post' action='/pair'><label for='pairing_code'>Pass code</label><input id='pairing_code' name='pairing_code' type='text' inputmode='numeric' autocomplete='one-time-code' maxlength='6' placeholder='Enter pass code from frame' required><button type='submit'>Validate this browser</button></form>",
                error_display = error_display,
                error_text = error_text,
        )
}

#[cfg(target_os = "espidf")]
fn render_top_action_html(session: &LocalSetupSession, sign_in_url: &str) -> String {
    let callout_class = if session.pairing_verified {
        "hero-callout"
    } else {
        "hero-callout hidden"
    };
    let waiting_class =
        if session.pairing_verified && session.browser_auth.authorization_url.is_none() {
            "hero-callout-body"
        } else {
            "hero-callout-body hidden"
        };
    let link_class = if session.pairing_verified {
        "hero-link"
    } else {
        "hero-link hidden"
    };
    let error_html = session
        .browser_auth
        .error
        .as_deref()
        .map(|message| {
            format!(
                "<p id='google-error' class='error'>{}</p>",
                escape_html(message)
            )
        })
        .unwrap_or_default();

    format!(
        "<section id='google-cta' class='{callout_class}'><p class='hero-callout-title'>Next step</p><p id='google-cta-copy' class='hero-callout-body'>Continue in this verified browser to approve Google Photos access for this frame.</p><a id='google-link' class='{link_class}' href='{href}'>Continue with Google Photos</a><p id='google-waiting' class='{waiting_class}'>Waiting for this browser to start Google Photos consent.</p>{error_html}</section>",
                callout_class = callout_class,
                link_class = link_class,
                waiting_class = waiting_class,
                href = escape_html(sign_in_url),
        error_html = error_html,
        )
}

#[cfg(target_os = "espidf")]
fn render_google_sign_in_html(session: &LocalSetupSession, sign_in_url: &str) -> String {
    if session
        .setup
        .owner_email
        .as_deref()
        .is_some_and(|value| !value.is_empty())
    {
        return String::new();
    }

    if session.pairing_verified {
        return format!(
            "<section class='tile tile-wide' id='google-sign-in-tile'><h2>Google Photos access</h2><p class='tile-copy'>This browser is verified. Open <a id='google-device-link-inline' href='{href}'>the Google consent step</a> here, approve Photos access, and the frame will pick it up automatically.</p><p class='tile-copy'>If Google returns to this page with an error, you can retry the consent step without restarting the frame.</p></section>",
                        href = escape_html(sign_in_url),
                );
    }

    "<section class='tile tile-wide' id='google-sign-in-tile'><h2>Google Photos access</h2><p class='tile-copy'>Google consent stays hidden until this browser proves it can see the pass code shown on the frame.</p></section>".to_string()
}

#[cfg(target_os = "espidf")]
fn render_album_selection_html(session: &LocalSetupSession) -> String {
    let Some(owner_email) = session
        .setup
        .owner_email
        .as_deref()
        .filter(|value| !value.is_empty())
    else {
        return String::new();
    };

    format!(
        "<section class='tile tile-wide' id='album-selection-tile'><h2>Choose album</h2><div class='album-shell'><p class='tile-copy'>The frame is signed in as <strong>{owner_email}</strong>. Browser-based Photos consent is now wired into setup, but the album picker itself is still the next implementation step.</p><div class='album-shell-card'><strong>What happens next</strong><p class='tile-copy'>The frame can now complete browser consent on-device and persist the resulting owner token. The next slice will use that Photos-scoped token to list albums here.</p></div></div></section>",
        owner_email = escape_html(owner_email),
    )
}

#[cfg(target_os = "espidf")]
fn render_local_setup_poll_script() -> &'static str {
    r#"<script>
const escapeHtml = (value) => String(value ?? '')
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#39;');

function updatePairingState(data) {
    const pairingState = document.getElementById('pairing-state');
    if (!pairingState) return;

    if (data.pairing_verified) {
        pairingState.innerHTML = "<p class='tile-copy'>This browser is verified against the pass code currently shown on the frame.</p>";
        return;
    }

    const errorText = data.pairing_error ? `<p class='error' id='pairing-error' style='display:block'>${escapeHtml(data.pairing_error)}</p>` : "<p class='error' id='pairing-error' style='display:none'></p>";
    pairingState.innerHTML = `<p class='tile-copy'>Enter the pass code shown on the frame. That code never appears on this webpage, which keeps nearby setup private.</p>${errorText}<form class='pair-form' method='post' action='/pair'><label for='pairing_code'>Pass code</label><input id='pairing_code' name='pairing_code' type='text' inputmode='numeric' autocomplete='one-time-code' maxlength='6' placeholder='Enter pass code from frame' required><button type='submit'>Validate this browser</button></form>`;
}

function updateGoogleCallout(data) {
    const callout = document.getElementById('google-cta');
    const link = document.getElementById('google-link');
    const waiting = document.getElementById('google-waiting');
    const copy = document.getElementById('google-cta-copy');
    const tile = document.getElementById('google-sign-in-tile');

    if (!callout || !waiting || !copy || !tile) return;

    if (data.owner_email) {
        callout.className = 'hero-callout hidden';
        if (tile) {
            tile.innerHTML = '';
        }
        return;
    }

    if (!data.pairing_verified) {
        callout.className = 'hero-callout hidden';
        if (tile) {
            tile.innerHTML = "<h2>Google sign-in</h2><p class='tile-copy'>Google sign-in stays hidden until this browser proves it can see the pass code shown on the frame.</p>";
        }
        return;
    }

    callout.className = 'hero-callout';
    copy.textContent = 'Continue in this verified browser to approve Google Photos access for this frame.';

    if (data.browser_auth_url) {
        link.className = 'hero-link';
        link.href = data.browser_auth_url;
        waiting.className = 'hero-callout-body hidden';
        tile.innerHTML = `<h2>Google Photos access</h2><p class='tile-copy'>This browser is verified. Open <a id='google-device-link-inline' href='${escapeHtml(data.browser_auth_url)}'>the Google consent step</a> here, approve Photos access, and the frame will pick it up automatically.</p><p class='tile-copy'>If Google returns to this page with an error, you can retry the consent step without restarting the frame.</p>`;
    } else {
        link.className = 'hero-link hidden';
        link.removeAttribute('href');
        waiting.className = 'hero-callout-body';
        tile.innerHTML = "<h2>Google Photos access</h2><p class='tile-copy'>This browser is verified. Waiting for Google Photos consent to become available.</p>";
    }

    let error = document.getElementById('google-error');
    if (data.browser_auth_error) {
        if (!error) {
            error = document.createElement('p');
            error.id = 'google-error';
            error.className = 'error';
            callout.appendChild(error);
        }
        error.textContent = data.browser_auth_error;
    } else if (error) {
        error.remove();
    }
}

function updateFromStatus(data) {
    const badge = document.getElementById('badge');
    const status = document.getElementById('status');
    const detail = document.getElementById('detail');
    const summaryStatus = document.getElementById('summary-status');
    const summaryDetail = document.getElementById('summary-detail');
    const localUrl = document.getElementById('local-url');
    const ipUrl = document.getElementById('ip-url');
    const ownerTile = document.getElementById('owner-tile');
    const ownerEmail = document.getElementById('owner-email');

    if (badge) {
        badge.textContent = data.owner_email ? 'Choose album' : (data.pairing_verified ? 'Google sign-in' : 'Local setup');
    }
    if (status) status.textContent = data.status || '';
    if (detail) detail.textContent = data.detail || '';
    if (summaryStatus) summaryStatus.textContent = data.status || '';
    if (summaryDetail) summaryDetail.textContent = data.detail || '';
    if (localUrl) localUrl.textContent = data.local_setup_url || 'Unavailable';
    if (ipUrl) ipUrl.textContent = data.local_setup_ip_url || 'Unavailable';

    if (ownerTile && ownerEmail) {
        if (data.owner_email) {
            ownerTile.style.display = 'block';
            ownerEmail.textContent = data.owner_email;
        } else {
            ownerTile.style.display = 'none';
            ownerEmail.textContent = '';
        }
    }

    updatePairingState(data);
    updateGoogleCallout(data);
}

async function pollStatus() {
    try {
        const response = await fetch('/status', { cache: 'no-store' });
        if (!response.ok) return;
        const data = await response.json();
        updateFromStatus(data);
    } catch (_) {
    }
}

pollStatus();
setInterval(pollStatus, 2000);
</script>"#
}

#[cfg(target_os = "espidf")]
fn apply_link_code_from_uri(session: &mut LocalSetupSession, request_uri: &str) {
    let Some((_, query)) = request_uri.split_once('?') else {
        return;
    };

    let params = parse_form_encoded(query);
    let Some(link_code) = params
        .get("link_code")
        .or_else(|| params.get("pairing_code"))
    else {
        return;
    };

    validate_pairing_code(
        session,
        link_code.trim(),
        "This QR link does not match the pairing code currently shown on the frame. Scan the latest QR code or type the pass code manually.",
    );
}

#[cfg(target_os = "espidf")]
fn validate_pairing_code(
    session: &mut LocalSetupSession,
    submitted_code: &str,
    invalid_message: &str,
) {
    let expected_code = session.setup.pairing_code.as_deref().unwrap_or_default();

    if expected_code.is_empty() {
        session.pairing_verified = false;
        session.browser_auth = BrowserAuthSession::default();
        session.pairing_error = Some(
            "The frame has not generated a pairing code yet. Wait for the device UI to finish loading."
                .to_string(),
        );
    } else if submitted_code == expected_code {
        session.pairing_verified = true;
        session.pairing_error = None;
    } else {
        session.pairing_verified = false;
        session.browser_auth = BrowserAuthSession::default();
        session.pairing_error = Some(invalid_message.to_string());
    }
}

#[cfg(target_os = "espidf")]
fn prepare_browser_authorization(session: &mut LocalSetupSession) -> Result<String> {
    if !session.pairing_verified {
        anyhow::bail!("browser pairing must be verified before starting Google Photos consent");
    }

    let redirect_uri = frame_api::oauth::resolve_browser_redirect_uri(
        session.setup.local_setup_url.as_deref(),
        session.setup.local_setup_ip_url.as_deref(),
    )?;
    let device_id = session
        .setup
        .device_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .context("browser OAuth device_id is missing from local setup state")?;
    let device_name = session
        .setup
        .device_name
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .context("browser OAuth device_name is missing from local setup state")?;
    let request = frame_api::oauth::build_browser_authorization_request(
        &redirect_uri,
        device_id,
        device_name,
    )?;

    session.browser_auth = BrowserAuthSession {
        authorization_url: Some(request.authorization_url.to_string()),
        redirect_uri: Some(request.redirect_uri.to_string()),
        state: Some(request.state),
        pkce_verifier: Some(request.pkce_verifier),
        access_token: None,
        error: None,
    };

    Ok(session
        .browser_auth
        .authorization_url
        .clone()
        .unwrap_or_default())
}

#[cfg(target_os = "espidf")]
fn finish_browser_authorization(session: &mut LocalSetupSession, request_uri: &str) {
    let Some((_, query)) = request_uri.split_once('?') else {
        session.browser_auth.error =
            Some("Google consent callback arrived without any query parameters.".to_string());
        return;
    };

    let params = parse_form_encoded(query);
    if let Some(error) = params.get("error") {
        session.browser_auth.access_token = None;
        session.browser_auth.error = Some(format!(
            "Google Photos consent was not completed: {}",
            error.replace('_', " ")
        ));
        return;
    }

    let Some(expected_state) = session.browser_auth.state.as_deref() else {
        session.browser_auth.error = Some(
            "Google consent returned, but the local setup session no longer has an active authorization request. Start the consent step again from the frame page."
                .to_string(),
        );
        return;
    };
    let Some(received_state) = params.get("state").map(String::as_str) else {
        session.browser_auth.error =
            Some("Google consent callback was missing the OAuth state parameter.".to_string());
        return;
    };
    if received_state != expected_state {
        session.browser_auth.access_token = None;
        session.browser_auth.error = Some("Google consent callback did not match the active local setup session. Start the consent step again from the frame page.".to_string());
        return;
    }

    let Some(code) = params.get("code").map(String::as_str) else {
        session.browser_auth.error =
            Some("Google consent callback was missing the authorization code.".to_string());
        return;
    };
    let Some(redirect_uri) = session.browser_auth.redirect_uri.as_deref() else {
        session.browser_auth.error =
            Some("Google consent callback could not be matched to a redirect URI.".to_string());
        return;
    };
    let Some(pkce_verifier) = session.browser_auth.pkce_verifier.as_deref() else {
        session.browser_auth.error = Some(
            "Google consent callback could not be matched to the active PKCE verifier.".to_string(),
        );
        return;
    };

    let redirect_uri = match url::Url::parse(redirect_uri) {
        Ok(uri) => uri,
        Err(error) => {
            session.browser_auth.error = Some(format!(
                "Stored browser OAuth redirect URI is invalid: {error}"
            ));
            return;
        }
    };

    match frame_api::oauth::exchange_browser_authorization_code(&redirect_uri, code, pkce_verifier)
    {
        Ok(token) => {
            session.browser_auth.access_token = Some(token);
            session.browser_auth.authorization_url = None;
            session.browser_auth.state = None;
            session.browser_auth.pkce_verifier = None;
            session.browser_auth.error = None;
        }
        Err(error) => {
            session.browser_auth.access_token = None;
            session.browser_auth.error = Some(format!(
                "Google Photos consent callback reached the frame, but exchanging the authorization code failed: {error:#}"
            ));
        }
    }
}

#[cfg(target_os = "espidf")]
fn render_browser_callback_html(session: &LocalSetupSession) -> String {
    let (title, detail) = if session.browser_auth.access_token.is_some() {
        (
            "Google Photos connected",
            "This browser finished consent successfully. You can return to the frame page while the device completes setup.",
        )
    } else {
        (
            "Google Photos connection incomplete",
            session
                .browser_auth
                .error
                .as_deref()
                .unwrap_or("Google consent did not complete successfully. Return to the frame page and retry the consent step."),
        )
    };

    format!(
        "<!doctype html><html><head><meta charset='utf-8'><meta name='viewport' content='width=device-width,initial-scale=1'><title>{title}</title><style>:root{{color-scheme:dark;--bg:#09111d;--panel:#0d1728;--border:#27415d;--text:#f8fafc;--muted:#b8c6d8;--accent:#7dd3fc}}*{{box-sizing:border-box}}body{{margin:0;min-height:100vh;display:grid;place-items:center;padding:24px;font-family:'Avenir Next','Segoe UI Variable','Segoe UI',sans-serif;background:radial-gradient(circle at top left,#14304a 0,transparent 28%),radial-gradient(circle at bottom right,#0f3b38 0,transparent 24%),var(--bg);color:var(--text)}}main{{width:min(100%,34rem);padding:28px;border-radius:28px;border:1px solid var(--border);background:linear-gradient(180deg,#10253a,#0d1728)}}h1{{margin:0 0 10px;font-size:2rem}}p{{margin:0;color:var(--muted);line-height:1.6}}a{{display:inline-flex;margin-top:18px;padding:14px 18px;border-radius:14px;background:var(--accent);color:#0b1220;text-decoration:none;font-weight:800}}</style></head><body><main><h1>{title}</h1><p>{detail}</p><a href='/'>Return to the frame</a></main></body></html>",
        title = escape_html(title),
        detail = escape_html(detail),
    )
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
