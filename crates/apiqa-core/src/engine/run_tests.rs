use chrono::Utc;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{header, method, path},
};

use super::*;
use crate::{ApiRequest, Auth, BodyKind, Environment, ExtractionRule, KeyValue, ResponseAssertion};

#[tokio::test]
async fn runs_and_persists_a_collection() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/health"))
        .and(header("authorization", "Bearer secret"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"ok":true,"user":{"id":"abc"}})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/users/abc"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"name":"Ada"})))
        .mount(&server)
        .await;
    let store = Store::open(":memory:").unwrap();
    let engine = ApiQaEngine::new(store);
    let collection = Collection {
        id: "c1".into(),
        name: "Demo".into(),
        variables: vec![],
        imported_at: Utc::now(),
        import_warnings: vec![],
        requests: vec![
            ApiRequest {
                id: "r1".into(),
                collection_id: "c1".into(),
                folder_path: vec![],
                name: "Health".into(),
                method: "GET".into(),
                url: format!("{}/health", server.uri()),
                headers: vec![],
                query: vec![],
                body_kind: BodyKind::None,
                body: None,
                auth: Auth::Bearer {
                    token: "{{token}}".into(),
                },
                assertions: vec![ResponseAssertion::StatusEquals {
                    expected: 200,
                    name: "healthy".into(),
                }],
                extractions: vec![ExtractionRule::JsonPath {
                    name: "userId".into(),
                    path: "$.user.id".into(),
                }],
                disabled: false,
            },
            ApiRequest {
                id: "r2".into(),
                collection_id: "c1".into(),
                folder_path: vec![],
                name: "User".into(),
                method: "GET".into(),
                url: format!("{}/users/{{{{userId}}}}", server.uri()),
                headers: vec![],
                query: vec![],
                body_kind: BodyKind::None,
                body: None,
                auth: Auth::None,
                assertions: vec![],
                extractions: vec![],
                disabled: false,
            },
        ],
    };
    engine.save_collection(&collection).unwrap();
    let run = engine
        .run_collection(
            &collection,
            RunOptions {
                environment: Some(Environment {
                    id: "e1".into(),
                    name: "Test".into(),
                    variables: vec![KeyValue {
                        key: "token".into(),
                        value: "secret".into(),
                        enabled: true,
                    }],
                }),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(run.state, RunState::Completed);
    assert_eq!(run.executions[0].response.as_ref().unwrap().status, 200);
    assert!(run.executions[0].assertions[0].passed);
    assert_eq!(run.executions[0].extractions[0].value, "abc");
    assert_eq!(run.executions.len(), 2);
}

#[tokio::test]
async fn invalid_proxy_terminalizes_started_run() {
    let store = Store::open(":memory:").unwrap();
    let engine = ApiQaEngine::new(store);
    let collection = Collection {
        id: "c1".into(),
        name: "Demo".into(),
        requests: vec![],
        variables: vec![],
        imported_at: Utc::now(),
        import_warnings: vec![],
    };
    engine.save_collection(&collection).unwrap();

    let error = engine
        .run_collection(
            &collection,
            RunOptions {
                proxy_url: Some("://invalid proxy".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();

    assert!(matches!(error, CoreError::Http(_)));
    let runs = engine.runs(Some(&collection.id)).unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].state, RunState::Failed);
    assert!(runs[0].completed_at.is_some());
}
