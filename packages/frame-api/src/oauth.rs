use anyhow::{anyhow, Context, Result};
use oauth2::{
    basic::BasicClient, device::DeviceAuthorizationRequest, reqwest::http_client, AuthUrl, ClientId,
    ClientSecret, DeviceAuthorizationUrl, Scope, TokenUrl,
};
use serde::Deserialize;
use std::time::{Duration, Instant};
use url::Url;

// The `DeviceAuthorizationResponse` contains the URLs and codes needed to prompt
// the user to authorize the app. We'll show the `user_code` and `verification_uri`
// to the user and poll for completion.
#[derive(Clone, Debug, Deserialize)]
pub struct DeviceAuthorizationResponse {
    pub device_code: String,
    pub user_code: String,
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

fn default_interval() -> u64 {
    5
}

// The `create_oauth_client` function will create a new `BasicClient` from the
// environment variables.
fn create_oauth_client() -> Result<BasicClient> {
    dotenvy::dotenv().ok();

    let client_id =
        std::env::var("GOOGLE_OAUTH_CLIENT_ID").context("GOOGLE_OAUTH_CLIENT_ID is not set")?;
    let client_secret =
        std::env::var("GOOGLE_OAUTH_CLIENT_SECRET").context("GOOGLE_OAUTH_CLIENT_SECRET is not set")?;
    let auth_url = AuthUrl::new("https://accounts.google.com/o/oauth2/v2/auth".to_string())?;
    let token_url = TokenUrl::new("https://oauth2.googleapis.com/token".to_string())?;
    let device_auth_url =
        DeviceAuthorizationUrl::new("https://oauth2.googleapis.com/device/code".to_string())?;

    let client = BasicClient::new(
        ClientId::new(client_id),
        Some(ClientSecret::new(client_secret)),
        auth_url,
        Some(token_url),
    )
    .set_device_authorization_url(device_auth_url);

    Ok(client)
}

// `request_device_authorization` will make a request to the device authorization
// endpoint and return the `DeviceAuthorizationResponse`.
pub fn request_device_authorization() -> Result<DeviceAuthorizationResponse> {
    let client = create_oauth_client()?;
    let scopes = vec![
        Scope::new("https://www.googleapis.com/auth/photoslibrary.readonly".to_string()),
        Scope::new("https://www.googleapis.com/auth/userinfo.profile".to_string()),
    ];

    let details = client
        .exchange_device_code()?
        .add_scopes(scopes)
        .request(http_client)?;

    Ok(DeviceAuthorizationResponse {
        device_code: details.device_code().secret().clone(),
        user_code: details.user_code().secret().clone(),
        verification_uri: Url::parse(details.verification_uri().as_str())?,
        expires_in: details.expires_in().as_secs(),
        interval: details.interval().as_secs(),
    })
}

// `poll_for_device_authorization` will poll the token endpoint until the user
// has authorized the app.
pub fn poll_for_device_authorization(
    auth_response: &DeviceAuthorizationResponse,
) -> Result<DeviceAccessToken> {
    let client = create_oauth_client()?;
    let expires_at = Instant::now() + Duration::from_secs(auth_response.expires_in);

    loop {
        if Instant::now() > expires_at {
            return Err(anyhow!("authorization expired"));
        }

        let token_result = client
            .exchange_device_access_token(
                &oauth2::device::DeviceCode::new(auth_response.device_code.clone()),
            )
            .request(http_client);

        match token_result {
            Ok(token) => {
                return Ok(DeviceAccessToken {
                    access_token: token.access_token().secret().clone(),
                    refresh_token: token.refresh_token().map(|t| t.secret().clone()),
                    expires_in: token.expires_in().unwrap_or_default().as_secs(),
                });
            }
            Err(e) => {
                if let Some(source) = e.source() {
                    if source.to_string().contains("authorization_pending") {
                        std::thread::sleep(Duration::from_secs(auth_response.interval));
                        continue;
                    }
                }
                return Err(anyhow!("failed to exchange device code: {}", e));
            }
        }
    }
}
