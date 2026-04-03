use frame_core::models::PhotoMetadata;
use crate::client::GooglePhotosClient;

#[async_trait::async_trait]
pub trait PhotosClient {
    async fn recent_photos(&self) -> anyhow::Result<Vec<PhotoMetadata>>;
}

pub fn create_photos_client(access_token: String) -> Box<dyn PhotosClient + Send + Sync> {
    Box::new(GooglePhotosClient::new(access_token))
}
