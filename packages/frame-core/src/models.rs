use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GoogleUser {
    pub email: String,
    pub subject: String,
    pub refresh_token: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PhotoMetadata {
    pub id: String,
    #[serde(rename = "baseUrl")]
    pub base_url: String,
    #[serde(flatten)]
    pub metadata: MediaMetadata,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaMetadata {
    pub width: String,
    pub height: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AlbumMetadata {
    pub id: String,
    pub title: String,
}
