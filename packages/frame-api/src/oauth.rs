use anyhow::{Context, Result, anyhow};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use frame_core::models::GoogleUser;
use rand::RngCore;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::time::{Duration, Instant};
use url::Url;

#[cfg(target_os = "espidf")]
use url::form_urlencoded;

#[cfg(target_os = "espidf")]
use embedded_svc::{
    http::{Method, client::Client as HttpClient},
    io::Write as _,
};
#[cfg(target_os = "espidf")]
use esp_idf_svc::http::client::{Configuration as HttpConfiguration, EspHttpConnection};
#[cfg(not(target_os = "espidf"))]
use oauth2::{
    AuthUrl, ClientId, ClientSecret, DeviceAuthorizationUrl, Scope,
    StandardDeviceAuthorizationResponse, TokenUrl, basic::BasicClient, reqwest,
};

// The `DeviceAuthorizationResponse` contains the URLs and codes needed to prompt
// the user to authorize the app. We'll show the `user_code` and `verification_uri`
// to the user and poll for completion.
#[derive(Clone, Debug, Deserialize)]
pub struct DeviceAuthorizationResponse {
    pub device_code: String,
    pub user_code: String,
    #[serde(alias = "verification_url")]
    pub verification_uri: Url,
    pub expires_in: u64,
    #[serde(default = "default_interval")]
    pub interval: u64,
}

// The `DeviceAccessToken` is the final token that we can use to make API calls.
#[derive(Clone, Debug, Deserialize)]
pub struct DeviceAccessToken {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: u64,
}

#[derive(Clone, Debug)]
pub struct BrowserAuthorizationRequest {
    pub authorization_url: Url,
    pub redirect_uri: Url,
    pub state: String,
    pub pkce_verifier: String,
    pub device_id: String,
    pub device_name: String,
}

#[cfg(target_os = "espidf")]
#[derive(Debug, Deserialize)]
struct OAuthErrorResponse {
    error: Option<String>,
    error_description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UserInfoResponse {
    email: String,
    sub: String,
}

fn default_interval() -> u64 {
    5
}

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

fn oauth_client_credentials() -> Result<(String, String)> {
    #[cfg(not(target_os = "espidf"))]
    dotenvy::dotenv().ok();

    let client_id = embedded_or_runtime_env(
        "GOOGLE_OAUTH_CLIENT_ID",
        option_env!("GOOGLE_OAUTH_CLIENT_ID"),
    )
    .context("GOOGLE_OAUTH_CLIENT_ID is not set")?;
    let client_secret = embedded_or_runtime_env(
        "GOOGLE_OAUTH_CLIENT_SECRET",
        option_env!("GOOGLE_OAUTH_CLIENT_SECRET"),
    )
    .context("GOOGLE_OAUTH_CLIENT_SECRET is not set")?;

    Ok((client_id, client_secret))
}

fn configured_browser_redirect_uri() -> Option<String> {
    embedded_or_runtime_env(
        "GOOGLE_OAUTH_REDIRECT_URI",
        option_env!("GOOGLE_OAUTH_REDIRECT_URI"),
    )
}

fn device_authorization_scopes() -> Vec<String> {
    // Google's limited-input device flow only supports a restricted scope set.
    // Google Photos library scopes are rejected here and must be requested from
    // a browser-capable local setup flow instead.
    vec![
        "openid".to_string(),
        "email".to_string(),
        "profile".to_string(),
    ]
}

fn browser_authorization_scopes() -> Vec<String> {
    vec![
        "openid".to_string(),
        "email".to_string(),
        "profile".to_string(),
        "https://www.googleapis.com/auth/photoslibrary.readonly".to_string(),
    ]
}

fn random_urlsafe_string(byte_len: usize) -> String {
    let mut bytes = vec![0_u8; byte_len];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn pkce_code_challenge(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

pub fn resolve_browser_redirect_uri(
    local_setup_url: Option<&str>,
    local_setup_ip_url: Option<&str>,
) -> Result<Url> {
    if let Some(configured) = configured_browser_redirect_uri() {
        return Url::parse(&configured)
            .with_context(|| format!("invalid GOOGLE_OAUTH_REDIRECT_URI: {configured}"));
    }

    let base = local_setup_url.or(local_setup_ip_url).context(
        "browser OAuth redirect URI is unavailable because the local setup URL is not ready",
    )?;
    let mut redirect_uri = Url::parse(base)
        .with_context(|| format!("invalid local setup URL for browser OAuth redirect: {base}"))?;
    redirect_uri.set_path("/oauth/callback");
    redirect_uri.set_query(None);
    redirect_uri.set_fragment(None);
    Ok(redirect_uri)
}

pub fn build_browser_authorization_request(
    redirect_uri: &Url,
    device_id: &str,
    device_name: &str,
) -> Result<BrowserAuthorizationRequest> {
    let (client_id, _) = oauth_client_credentials()?;
    let device_id = device_id.trim();
    let device_name = device_name.trim();

    if device_id.is_empty() {
        anyhow::bail!("browser OAuth requires a non-empty device_id for private-IP Google login");
    }
    if device_name.is_empty() {
        anyhow::bail!("browser OAuth requires a non-empty device_name for private-IP Google login");
    }

    let state = random_urlsafe_string(24);
    let pkce_verifier = random_urlsafe_string(48);
    let code_challenge = pkce_code_challenge(&pkce_verifier);
    let scope = browser_authorization_scopes().join(" ");
    let mut authorization_url = Url::parse("https://accounts.google.com/o/oauth2/v2/auth")
        .context("invalid Google authorization endpoint")?;

    authorization_url
        .query_pairs_mut()
        .append_pair("client_id", &client_id)
        .append_pair("redirect_uri", redirect_uri.as_str())
        .append_pair("response_type", "code")
        .append_pair("scope", &scope)
        .append_pair("state", &state)
        .append_pair("code_challenge", &code_challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("access_type", "offline")
        .append_pair("include_granted_scopes", "true")
        .append_pair("prompt", "consent")
        .append_pair("device_id", device_id)
        .append_pair("device_name", device_name);

    Ok(BrowserAuthorizationRequest {
        authorization_url,
        redirect_uri: redirect_uri.clone(),
        state,
        pkce_verifier,
        device_id: device_id.to_string(),
        device_name: device_name.to_string(),
    })
}

#[cfg(target_os = "espidf")]
fn form_body(pairs: &[(&str, &str)]) -> String {
    let mut serializer = form_urlencoded::Serializer::new(String::new());
    for (key, value) in pairs {
        serializer.append_pair(key, value);
    }
    serializer.finish()
}

#[cfg(not(target_os = "espidf"))]
fn build_oauth_http_client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::ClientBuilder::new()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .context("failed to build OAuth HTTP client")
}

#[cfg(target_os = "espidf")]
fn oauth_request(url: &str, body: &str) -> Result<(u16, String)> {
    oauth_request_with_method(Method::Post, url, Some(body), &[])
}

#[cfg(target_os = "espidf")]
fn oauth_request_with_method(
    method: Method,
    url: &str,
    body: Option<&str>,
    extra_headers: &[(&str, &str)],
) -> Result<(u16, String)> {
    let http_config = HttpConfiguration {
        crt_bundle_attach: Some(esp_idf_svc::sys::esp_crt_bundle_attach),
        ..Default::default()
    };
    let connection =
        EspHttpConnection::new(&http_config).context("failed to build OAuth HTTP client")?;
    let mut client = HttpClient::wrap(connection);

    let content_length = body.map(str::len).unwrap_or(0).to_string();
    let mut headers = vec![("accept", "application/json")];

    if body.is_some() {
        headers.push(("content-type", "application/x-www-form-urlencoded"));
        headers.push(("content-length", content_length.as_str()));
    }

    headers.extend_from_slice(extra_headers);

    let mut request = client
        .request(method, url, &headers)
        .with_context(|| format!("failed to open OAuth request to {url}"))?;

    if let Some(body) = body {
        request
            .write_all(body.as_bytes())
            .with_context(|| format!("failed to write OAuth request body to {url}"))?;
        request
            .flush()
            .with_context(|| format!("failed to flush OAuth request body to {url}"))?;
    }

    let mut response = request
        .submit()
        .with_context(|| format!("failed to send OAuth request to {url}"))?;
    let status = response.status();
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 1024];

    loop {
        let read = response
            .read(&mut chunk)
            .with_context(|| format!("failed to read OAuth response from {url}"))?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..read]);
    }

    let body = String::from_utf8(bytes).context("OAuth response payload was not valid UTF-8")?;
    Ok((status, body))
}

#[cfg(not(target_os = "espidf"))]
pub fn fetch_account_profile(access_token: &str) -> Result<GoogleUser> {
    let http_client = build_oauth_http_client()?;
    let response = http_client
        .get("https://openidconnect.googleapis.com/v1/userinfo")
        .bearer_auth(access_token)
        .send()
        .context("failed to fetch account profile from Google userinfo")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        return Err(anyhow!(
            "failed to fetch account profile, status {status}: {body}"
        ));
    }

    let payload = response
        .json::<UserInfoResponse>()
        .context("invalid account profile payload")?;

    Ok(GoogleUser {
        email: payload.email,
        subject: payload.sub,
        refresh_token: "".to_string(),
    })
}

#[cfg(target_os = "espidf")]
pub fn fetch_account_profile(access_token: &str) -> Result<GoogleUser> {
    let authorization = format!("Bearer {access_token}");
    let (status, response_body) = oauth_request_with_method(
        Method::Get,
        "https://openidconnect.googleapis.com/v1/userinfo",
        None,
        &[("authorization", authorization.as_str())],
    )
    .context("failed to fetch account profile from Google userinfo")?;

    if !(200..300).contains(&status) {
        return Err(anyhow!(
            "failed to fetch account profile, status {status}: {response_body}"
        ));
    }

    let payload = serde_json::from_str::<UserInfoResponse>(&response_body)
        .context("invalid account profile payload")?;

    Ok(GoogleUser {
        email: payload.email,
        subject: payload.sub,
        refresh_token: "".to_string(),
    })
}

#[cfg(not(target_os = "espidf"))]
pub fn refresh_device_access_token(refresh_token: &str) -> Result<DeviceAccessToken> {
    let (client_id, client_secret) = oauth_client_credentials()?;
    let http_client = build_oauth_http_client()?;
    let response = http_client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
        ])
        .send()
        .context("failed to refresh OAuth device token")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        return Err(anyhow!(
            "failed to refresh OAuth device token, status {status}: {body}"
        ));
    }

    response
        .json::<DeviceAccessToken>()
        .context("invalid OAuth refresh token payload")
}

#[cfg(target_os = "espidf")]
pub fn refresh_device_access_token(refresh_token: &str) -> Result<DeviceAccessToken> {
    let (client_id, client_secret) = oauth_client_credentials()?;
    let body = form_body(&[
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", client_id.as_str()),
        ("client_secret", client_secret.as_str()),
    ]);
    let (status, response_body) = oauth_request("https://oauth2.googleapis.com/token", &body)
        .context("failed to refresh OAuth device token")?;

    if !(200..300).contains(&status) {
        return Err(anyhow!(
            "failed to refresh OAuth device token, status {status}: {response_body}"
        ));
    }

    serde_json::from_str(&response_body).context("invalid OAuth refresh token payload")
}

#[cfg(not(target_os = "espidf"))]
pub fn exchange_browser_authorization_code(
    redirect_uri: &Url,
    authorization_code: &str,
    pkce_verifier: &str,
) -> Result<DeviceAccessToken> {
    let (client_id, client_secret) = oauth_client_credentials()?;
    let http_client = build_oauth_http_client()?;
    let response = http_client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", authorization_code),
            ("redirect_uri", redirect_uri.as_str()),
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("code_verifier", pkce_verifier),
        ])
        .send()
        .context("failed to exchange OAuth authorization code")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        return Err(anyhow!(
            "failed to exchange OAuth authorization code, status {status}: {body}"
        ));
    }

    response
        .json::<DeviceAccessToken>()
        .context("invalid OAuth authorization code token payload")
}

#[cfg(target_os = "espidf")]
pub fn exchange_browser_authorization_code(
    redirect_uri: &Url,
    authorization_code: &str,
    pkce_verifier: &str,
) -> Result<DeviceAccessToken> {
    let (client_id, client_secret) = oauth_client_credentials()?;
    let body = form_body(&[
        ("grant_type", "authorization_code"),
        ("code", authorization_code),
        ("redirect_uri", redirect_uri.as_str()),
        ("client_id", client_id.as_str()),
        ("client_secret", client_secret.as_str()),
        ("code_verifier", pkce_verifier),
    ]);
    let (status, response_body) = oauth_request("https://oauth2.googleapis.com/token", &body)
        .context("failed to exchange OAuth authorization code")?;

    if !(200..300).contains(&status) {
        return Err(anyhow!(
            "failed to exchange OAuth authorization code, status {status}: {response_body}"
        ));
    }

    serde_json::from_str(&response_body).context("invalid OAuth authorization code token payload")
}

// `request_device_authorization` will make a request to the device authorization
// endpoint and return the `DeviceAuthorizationResponse`.
#[cfg(not(target_os = "espidf"))]
pub fn request_device_authorization() -> Result<DeviceAuthorizationResponse> {
    let (client_id, client_secret) = oauth_client_credentials()?;
    let auth_url = AuthUrl::new("https://accounts.google.com/o/oauth2/v2/auth".to_string())?;
    let token_url = TokenUrl::new("https://oauth2.googleapis.com/token".to_string())?;
    let device_auth_url =
        DeviceAuthorizationUrl::new("https://oauth2.googleapis.com/device/code".to_string())?;

    let client = BasicClient::new(ClientId::new(client_id))
        .set_client_secret(ClientSecret::new(client_secret))
        .set_auth_uri(auth_url)
        .set_token_uri(token_url)
        .set_device_authorization_url(device_auth_url);

    let scopes = device_authorization_scopes()
        .into_iter()
        .map(Scope::new)
        .collect::<Vec<_>>();

    let http_client = build_oauth_http_client()?;

    let details: StandardDeviceAuthorizationResponse = client
        .exchange_device_code()
        .add_scopes(scopes)
        .request(&http_client)?;

    Ok(DeviceAuthorizationResponse {
        device_code: details.device_code().secret().clone(),
        user_code: details.user_code().secret().clone(),
        verification_uri: Url::parse(details.verification_uri().as_str())?,
        expires_in: details.expires_in().as_secs(),
        interval: details.interval().as_secs(),
    })
}

#[cfg(target_os = "espidf")]
pub fn request_device_authorization() -> Result<DeviceAuthorizationResponse> {
    let (client_id, _) = oauth_client_credentials()?;
    let scope = device_authorization_scopes().join(" ");
    let body = form_body(&[("client_id", client_id.as_str()), ("scope", scope.as_str())]);
    let (status, response_body) =
        oauth_request("https://oauth2.googleapis.com/device/code", &body)?;

    if !(200..300).contains(&status) {
        return Err(anyhow!(
            "OAuth device authorization request failed with status {status}: {response_body}"
        ));
    }

    serde_json::from_str(&response_body).context("invalid OAuth device authorization payload")
}

// `poll_for_device_authorization` will poll the token endpoint until the user
// has authorized the app.
#[cfg(not(target_os = "espidf"))]
pub fn poll_for_device_authorization(
    auth_response: &DeviceAuthorizationResponse,
) -> Result<DeviceAccessToken> {
    let (client_id, client_secret) = oauth_client_credentials()?;
    let http_client = build_oauth_http_client()?;

    let expires_at = Instant::now() + Duration::from_secs(auth_response.expires_in);

    loop {
        if Instant::now() > expires_at {
            return Err(anyhow!("authorization expired"));
        }

        let response = http_client
            .post("https://oauth2.googleapis.com/token")
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("device_code", auth_response.device_code.as_str()),
                ("client_id", client_id.as_str()),
                ("client_secret", client_secret.as_str()),
            ])
            .send()
            .context("failed to poll OAuth device token endpoint")?;

        if response.status().is_success() {
            let token = response
                .json::<DeviceAccessToken>()
                .context("invalid OAuth token response payload")?;
            return Ok(token);
        }

        let error_body = response.text().unwrap_or_default();
        if error_body.contains("authorization_pending") {
            std::thread::sleep(Duration::from_secs(auth_response.interval));
            continue;
        }

        if error_body.contains("slow_down") {
            std::thread::sleep(Duration::from_secs(auth_response.interval + 5));
            continue;
        }

        return Err(anyhow!(
            "failed to exchange device code, response: {error_body}"
        ));
    }
}

#[cfg(target_os = "espidf")]
pub fn poll_for_device_authorization(
    auth_response: &DeviceAuthorizationResponse,
) -> Result<DeviceAccessToken> {
    let (client_id, client_secret) = oauth_client_credentials()?;
    let expires_at = Instant::now() + Duration::from_secs(auth_response.expires_in);

    loop {
        if Instant::now() > expires_at {
            return Err(anyhow!("authorization expired"));
        }

        let body = form_body(&[
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("device_code", auth_response.device_code.as_str()),
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
        ]);
        let (status, response_body) = oauth_request("https://oauth2.googleapis.com/token", &body)
            .context("failed to poll OAuth device token endpoint")?;

        if (200..300).contains(&status) {
            return serde_json::from_str(&response_body)
                .context("invalid OAuth token response payload");
        }

        let oauth_error = serde_json::from_str::<OAuthErrorResponse>(&response_body).ok();
        let error_code = oauth_error
            .as_ref()
            .and_then(|payload| payload.error.as_deref())
            .unwrap_or_default();

        if error_code == "authorization_pending" {
            std::thread::sleep(Duration::from_secs(auth_response.interval));
            continue;
        }

        if error_code == "slow_down" {
            std::thread::sleep(Duration::from_secs(auth_response.interval + 5));
            continue;
        }

        let description = oauth_error
            .and_then(|payload| payload.error_description)
            .unwrap_or(response_body);

        return Err(anyhow!(
            "failed to exchange device code, status {status}: {description}"
        ));
    }
}
