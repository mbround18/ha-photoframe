use anyhow::Context;
use frame_core::models::{AlbumMetadata, PhotoMetadata};
#[cfg(not(target_os = "espidf"))]
use reqwest::Client as HttpClient;
use serde::Deserialize;

#[cfg(target_os = "espidf")]
use embedded_svc::{
    http::{Method, client::Client as HttpClient},
    io::Write as _,
};
#[cfg(target_os = "espidf")]
use esp_idf_svc::http::client::{Configuration as HttpConfiguration, EspHttpConnection};

const API_BASE_URL: &str = "https://photoslibrary.googleapis.com/v1";

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ListAlbumsResponse {
    albums: Vec<AlbumMetadata>,
    next_page_token: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct SearchMediaItemsResponse {
    media_items: Vec<PhotoMetadata>,
    next_page_token: Option<String>,
}

pub struct GooglePhotosClient {
    #[cfg(not(target_os = "espidf"))]
    http_client: HttpClient,
    access_token: String,
    base_url: String,
}

impl GooglePhotosClient {
    pub fn new(access_token: String) -> Self {
        Self {
            #[cfg(not(target_os = "espidf"))]
            http_client: HttpClient::new(),
            access_token,
            base_url: API_BASE_URL.to_string(),
        }
    }

    pub fn with_base_url(mut self, base_url: String) -> Self {
        self.base_url = base_url;
        self
    }

    #[cfg(not(target_os = "espidf"))]
    pub async fn list_albums(&self) -> anyhow::Result<Vec<AlbumMetadata>> {
        let url = format!("{}/albums", self.base_url);
        let mut albums = Vec::new();
        let mut next_page_token: Option<String> = None;

        loop {
            let mut request = self.http_client.get(&url);
            if let Some(token) = &next_page_token {
                request = request.query(&[("pageToken", token)]);
            }

            let response = request
                .bearer_auth(&self.access_token)
                .send()
                .await
                .context("failed to send list_albums request")?;

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                anyhow::bail!("list_albums request failed with status {status}: {body}");
            }

            let mut album_response = response
                .json::<ListAlbumsResponse>()
                .await
                .context("failed to deserialize list_albums response")?;

            albums.append(&mut album_response.albums);

            if album_response.next_page_token.is_some() {
                next_page_token = album_response.next_page_token;
            } else {
                break;
            }
        }

        Ok(albums)
    }

    #[cfg(target_os = "espidf")]
    pub fn list_albums(&self) -> anyhow::Result<Vec<AlbumMetadata>> {
        let url = format!("{}/albums", self.base_url);
        let mut albums = Vec::new();
        let mut next_page_token: Option<String> = None;

        loop {
            let page_url = if let Some(token) = &next_page_token {
                format!("{url}?pageToken={token}")
            } else {
                url.clone()
            };

            let response_body = self.request(Method::Get, &page_url, None)?;
            let mut album_response = serde_json::from_str::<ListAlbumsResponse>(&response_body)
                .context("failed to deserialize list_albums response")?;

            albums.append(&mut album_response.albums);

            if album_response.next_page_token.is_some() {
                next_page_token = album_response.next_page_token;
            } else {
                break;
            }
        }

        Ok(albums)
    }

    #[cfg(not(target_os = "espidf"))]
    pub async fn get_photos(&self, album_id: &str) -> anyhow::Result<Vec<PhotoMetadata>> {
        let url = format!("{}/mediaItems:search", self.base_url);
        let mut photos = Vec::new();
        let mut next_page_token: Option<String> = None;

        loop {
            let mut request = self.http_client.post(&url);
            if let Some(token) = &next_page_token {
                request = request.query(&[("pageToken", token)]);
            }

            let response = request
                .bearer_auth(&self.access_token)
                .json(&serde_json::json!({
                    "albumId": album_id,
                    "pageSize": 100
                }))
                .send()
                .await
                .context("failed to send get_photos request")?;

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                anyhow::bail!("get_photos request failed with status {status}: {body}");
            }

            let mut photo_response = response
                .json::<SearchMediaItemsResponse>()
                .await
                .context("failed to deserialize get_photos response")?;

            photos.append(&mut photo_response.media_items);

            if photo_response.next_page_token.is_some() {
                next_page_token = photo_response.next_page_token;
            } else {
                break;
            }
        }

        Ok(photos)
    }

    #[cfg(target_os = "espidf")]
    pub fn get_photos(&self, album_id: &str) -> anyhow::Result<Vec<PhotoMetadata>> {
        let url = format!("{}/mediaItems:search", self.base_url);
        let mut photos = Vec::new();
        let mut next_page_token: Option<String> = None;

        loop {
            let page_url = if let Some(token) = &next_page_token {
                format!("{url}?pageToken={token}")
            } else {
                url.clone()
            };
            let body = serde_json::json!({
                "albumId": album_id,
                "pageSize": 100,
            })
            .to_string();

            let response_body = self.request(Method::Post, &page_url, Some(body.as_str()))?;
            let mut photo_response =
                serde_json::from_str::<SearchMediaItemsResponse>(&response_body)
                    .context("failed to deserialize get_photos response")?;

            photos.append(&mut photo_response.media_items);

            if photo_response.next_page_token.is_some() {
                next_page_token = photo_response.next_page_token;
            } else {
                break;
            }
        }

        Ok(photos)
    }

    #[cfg(not(target_os = "espidf"))]
    pub async fn list_recent_photos(&self) -> anyhow::Result<Vec<PhotoMetadata>> {
        Ok(Vec::new())
    }

    #[cfg(target_os = "espidf")]
    pub fn list_recent_photos(&self) -> anyhow::Result<Vec<PhotoMetadata>> {
        Ok(Vec::new())
    }

    #[cfg(target_os = "espidf")]
    fn request(&self, method: Method, url: &str, body: Option<&str>) -> anyhow::Result<String> {
        let http_config = HttpConfiguration {
            crt_bundle_attach: Some(esp_idf_svc::sys::esp_crt_bundle_attach),
            ..Default::default()
        };
        let connection = EspHttpConnection::new(&http_config)
            .context("failed to build Google Photos HTTP client")?;
        let mut client = HttpClient::wrap(connection);

        let authorization = format!("Bearer {}", self.access_token);
        let content_length = body.map(str::len).unwrap_or(0).to_string();
        let mut headers = vec![
            ("accept", "application/json"),
            ("authorization", authorization.as_str()),
        ];

        if body.is_some() {
            headers.push(("content-type", "application/json"));
            headers.push(("content-length", content_length.as_str()));
        }

        let mut request = client
            .request(method, url, &headers)
            .with_context(|| format!("failed to open Google Photos request to {url}"))?;

        if let Some(body) = body {
            request
                .write_all(body.as_bytes())
                .with_context(|| format!("failed to write Google Photos request body to {url}"))?;
            request
                .flush()
                .with_context(|| format!("failed to flush Google Photos request body to {url}"))?;
        }

        let mut response = request
            .submit()
            .with_context(|| format!("failed to send Google Photos request to {url}"))?;
        let status = response.status();
        let mut bytes = Vec::new();
        let mut chunk = [0_u8; 1024];

        loop {
            let read = response
                .read(&mut chunk)
                .with_context(|| format!("failed to read Google Photos response from {url}"))?;
            if read == 0 {
                break;
            }
            bytes.extend_from_slice(&chunk[..read]);
        }

        let body = String::from_utf8(bytes)
            .context("Google Photos response payload was not valid UTF-8")?;

        if !(200..300).contains(&status) {
            anyhow::bail!("Google Photos request failed with status {status}: {body}");
        }

        Ok(body)
    }
}

#[async_trait::async_trait]
impl crate::transport::PhotosClient for GooglePhotosClient {
    #[cfg(not(target_os = "espidf"))]
    async fn recent_photos(&self) -> anyhow::Result<Vec<PhotoMetadata>> {
        self.list_recent_photos().await
    }

    #[cfg(target_os = "espidf")]
    async fn recent_photos(&self) -> anyhow::Result<Vec<PhotoMetadata>> {
        self.list_recent_photos()
    }
}
