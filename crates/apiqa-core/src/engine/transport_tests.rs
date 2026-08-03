use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

use super::*;
use crate::{Auth, BodyKind};

fn request(url: String) -> ApiRequest {
    ApiRequest {
        id: "r1".into(),
        collection_id: "c1".into(),
        folder_path: vec![],
        name: "Request".into(),
        method: "GET".into(),
        url,
        headers: vec![],
        query: vec![],
        body_kind: BodyKind::None,
        body: None,
        auth: Auth::None,
        assertions: vec![],
        extractions: vec![],
        disabled: false,
    }
}

#[tokio::test]
async fn streams_and_truncates_oversized_response() {
    let server = MockServer::start().await;
    let body = vec![b'x'; MAX_CAPTURED_BODY_SIZE + 4096];
    Mock::given(method("GET"))
        .and(path("/large"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body.clone()))
        .mount(&server)
        .await;

    let response = send(
        &Client::new(),
        &request(format!("{}/large", server.uri())),
        &HashMap::new(),
    )
    .await
    .unwrap();

    assert_eq!(response.body.len(), MAX_CAPTURED_BODY_SIZE);
    assert_eq!(response.body_size, body.len() as u64);
    assert!(response.truncated);
}

#[tokio::test]
async fn invalid_request_is_typed_as_configuration_failure() {
    let error = send(
        &Client::new(),
        &request("file:///tmp/data".into()),
        &HashMap::new(),
    )
    .await
    .unwrap_err();

    assert!(matches!(error, RequestError::Invalid(_)));
}
