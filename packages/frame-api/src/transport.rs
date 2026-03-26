use frame_core::models::PhotoMetadata;

use crate::client::GooglePhotosClient;

pub trait PhotosClient {
    fn recent_photos(&self) -> anyhow::Result<Vec<PhotoMetadata>>;
}

pub fn create_photos_client() -> Box<dyn PhotosClient> {
    Box::new(GooglePhotosClient::new())
}
