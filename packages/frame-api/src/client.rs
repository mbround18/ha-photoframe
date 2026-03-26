use frame_core::models::PhotoMetadata;

pub struct GooglePhotosClient;

impl GooglePhotosClient {
    pub fn new() -> Self {
        Self
    }

    pub fn list_recent_photos(&self) -> anyhow::Result<Vec<PhotoMetadata>> {
        Ok(Vec::new())
    }
}

impl Default for GooglePhotosClient {
    fn default() -> Self {
        Self::new()
    }
}

impl crate::transport::PhotosClient for GooglePhotosClient {
    fn recent_photos(&self) -> anyhow::Result<Vec<PhotoMetadata>> {
        self.list_recent_photos()
    }
}
