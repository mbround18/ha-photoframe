use frame_api::client::GooglePhotosClient;
use wiremock::{Mock, MockServer, ResponseTemplate};
use wiremock::matchers::{method, path};

#[tokio::test]
async fn test_list_albums() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/albums"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "albums": [{"id": "1", "title": "test"}]
        })))
        .mount(&server)
        .await;

    let client = GooglePhotosClient::new("test_token".to_string()).with_base_url(server.uri());
    let albums = client.list_albums().await.unwrap();

    assert_eq!(albums.len(), 1);
    assert_eq!(albums[0].id, "1");
    assert_eq!(albums[0].title, "test");
}

#[tokio::test]
async fn test_get_photos() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/albums"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "albums": [{"id": "1", "title": "test"}]
        })))
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/mediaItems:search"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "mediaItems": [{"id": "photo1", "baseUrl": "https://example.com/photo1.jpg", "width": "1024", "height": "768"}]
        })))
        .mount(&server)
        .await;

    let client = GooglePhotosClient::new("test_token".to_string()).with_base_url(server.uri());
    let albums = client.list_albums().await.unwrap();
    let album_id = albums.first().unwrap().id.clone();
    let photos = client.get_photos(&album_id).await.unwrap();

    assert_eq!(photos.len(), 1);
    assert_eq!(photos[0].id, "photo1");
    assert_eq!(photos[0].base_url, "https://example.com/photo1.jpg");
    assert_eq!(photos[0].metadata.width, "1024");
    assert_eq!(photos[0].metadata.height, "768");
}
