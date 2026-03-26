// This is the root of the frame-api crate.
// It will handle Google Photos REST API integration, OAuth2, and image downloading.

pub mod client;
pub mod oauth;
pub mod transport;

pub use client::GooglePhotosClient;
pub use oauth::DeviceAuthorization;
pub use transport::{PhotosClient, create_photos_client};
