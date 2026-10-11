#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! HelixClient behaviour against a mock Twitch: pagination, refresh-on-401,
//! rate limiting and malformed responses.

use std::sync::Arc;

use lurkn_auth::storage::{MemoryStorage, TokenStorage};
use lurkn_auth::{IdEndpoints, Session, TokenSet};
use lurkn_twitch::client::Page;
use lurkn_twitch::{HelixClient, HelixError};
use serde::Deserialize;
use serde_json::json;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const CLIENT_ID: &str = "testclientid0000000000000000000";
const SCOPES: [&str; 3] = ["user:read:follows", "user:read:chat", "user:write:chat"];

#[derive(Debug, Deserialize)]
struct Row {
    id: String,
}

fn scopes() -> Vec<String> {
    SCOPES.iter().map(|s| (*s).to_owned()).collect()
}

/// A session holding a token that is still fresh, so no refresh is attempted
/// unless a 401 forces one.
fn fresh_session(id_server: &MockServer, access: &str) -> Arc<Session> {
    let storage = MemoryStorage::default();
    storage
        .store(&TokenSet {
            access_token: access.into(),
            refresh_token: "refresh1".into(),
            obtained_at: time::OffsetDateTime::now_utc(),
            expires_in: time::Duration::seconds(14820),
            scopes: scopes(),
        })
        .unwrap();

    Arc::new(Session::new(
        reqwest::Client::new(),
        IdEndpoints::new(&id_server.uri()).unwrap(),
        CLIENT_ID.to_owned(),
        Box::new(storage),
    ))
}

/// `restore` loads the token into memory without hitting the network for a
/// refresh, but it does validate — so the id server needs that route.
async fn mount_validate(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/oauth2/validate"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "client_id": CLIENT_ID, "login": "testuser", "user_id": "12345",
            "scopes": SCOPES, "expires_in": 14820,
        })))
        .mount(server)
        .await;
}

fn client(helix: &MockServer, session: Arc<Session>) -> HelixClient {
    let _ = helix;
    HelixClient::new(reqwest::Client::new(), session, CLIENT_ID.to_owned())
}

fn url(server: &MockServer, p: &str) -> url::Url {
    url::Url::parse(&format!("{}{p}", server.uri())).unwrap()
}

#[tokio::test]
async fn sends_the_client_id_and_bearer_token() {
    let id_server = MockServer::start().await;
    mount_validate(&id_server).await;
    let session = fresh_session(&id_server, "access1");
    session.restore().await.unwrap().unwrap();

    let helix = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rows"))
        .and(header("Client-Id", CLIENT_ID))
        .and(header("Authorization", "Bearer access1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": [{"id": "a"}] })))
        .mount(&helix)
        .await;

    let page: Page<Row> = client(&helix, session)
        .get(&url(&helix, "/rows"))
        .await
        .unwrap();
    assert_eq!(page.data.len(), 1);
    assert_eq!(page.data[0].id, "a");
}

#[tokio::test]
async fn follows_the_cursor_until_twitch_stops_sending_one() {
    let id_server = MockServer::start().await;
    mount_validate(&id_server).await;
    let session = fresh_session(&id_server, "access1");
    session.restore().await.unwrap().unwrap();

    let helix = MockServer::start().await;
    // Page 1 hands back a cursor...
    Mock::given(method("GET"))
        .and(path("/rows"))
        .and(query_param("after", "CURSOR1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [{"id": "b"}], "pagination": {}
        })))
        .mount(&helix)
        .await;
    // ...the cursor-less first request is the entry point.
    Mock::given(method("GET"))
        .and(path("/rows"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [{"id": "a"}], "pagination": {"cursor": "CURSOR1"}
        })))
        .mount(&helix)
        .await;

    let rows: Vec<Row> = client(&helix, session)
        .get_all(
            |cursor| {
                let mut u = url(&helix, "/rows");
                if let Some(c) = cursor {
                    u.query_pairs_mut().append_pair("after", c);
                }
                u
            },
            10,
        )
        .await
        .unwrap();

    let ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(ids, ["a", "b"], "both pages, in order");
}

/// The clock-based refresh cannot catch a token revoked mid-session, so a 401
/// must trigger exactly one refresh-and-retry.
#[tokio::test]
async fn a_401_refreshes_once_and_retries() {
    let id_server = MockServer::start().await;
    mount_validate(&id_server).await;
    Mock::given(method("POST"))
        .and(path("/oauth2/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "access2", "refresh_token": "refresh2",
            "expires_in": 14820, "scope": SCOPES,
        })))
        .mount(&id_server)
        .await;

    let session = fresh_session(&id_server, "access1");
    session.restore().await.unwrap().unwrap();

    let helix = MockServer::start().await;
    // The stale token is rejected...
    Mock::given(method("GET"))
        .and(path("/rows"))
        .and(header("Authorization", "Bearer access1"))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({
            "status": 401, "message": "Invalid OAuth token"
        })))
        .mount(&helix)
        .await;
    // ...and the refreshed one works.
    Mock::given(method("GET"))
        .and(path("/rows"))
        .and(header("Authorization", "Bearer access2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "data": [{"id": "ok"}] })))
        .mount(&helix)
        .await;

    let page: Page<Row> = client(&helix, session)
        .get(&url(&helix, "/rows"))
        .await
        .unwrap();
    assert_eq!(page.data[0].id, "ok");
}

#[tokio::test]
async fn a_401_that_survives_the_refresh_demands_a_new_login() {
    let id_server = MockServer::start().await;
    mount_validate(&id_server).await;
    Mock::given(method("POST"))
        .and(path("/oauth2/token"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "status": 400, "message": "Invalid refresh token"
        })))
        .mount(&id_server)
        .await;

    let session = fresh_session(&id_server, "access1");
    session.restore().await.unwrap().unwrap();

    let helix = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rows"))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({
            "status": 401, "message": "Invalid OAuth token"
        })))
        .mount(&helix)
        .await;

    let error = client(&helix, session)
        .get::<Page<Row>>(&url(&helix, "/rows"))
        .await
        .unwrap_err();
    assert!(matches!(error, HelixError::Unauthorized), "got {error:?}");
    assert!(error.requires_reauth());
}

#[tokio::test]
async fn a_429_reports_when_to_retry() {
    let id_server = MockServer::start().await;
    mount_validate(&id_server).await;
    let session = fresh_session(&id_server, "access1");
    session.restore().await.unwrap().unwrap();

    let reset = time::OffsetDateTime::now_utc().unix_timestamp() + 42;
    let helix = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rows"))
        .respond_with(
            ResponseTemplate::new(429).insert_header("Ratelimit-Reset", reset.to_string().as_str()),
        )
        .mount(&helix)
        .await;

    let error = client(&helix, session)
        .get::<Page<Row>>(&url(&helix, "/rows"))
        .await
        .unwrap_err();
    match error {
        HelixError::RateLimited { retry_after_secs } => {
            assert!(
                (40..=43).contains(&retry_after_secs),
                "got {retry_after_secs}"
            );
        }
        other => panic!("expected RateLimited, got {other:?}"),
    }
    assert!(HelixError::RateLimited {
        retry_after_secs: 42
    }
    .is_transient());
}

#[tokio::test]
async fn a_server_fault_is_transient_so_stale_cache_can_be_served() {
    let id_server = MockServer::start().await;
    mount_validate(&id_server).await;
    let session = fresh_session(&id_server, "access1");
    session.restore().await.unwrap().unwrap();

    let helix = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rows"))
        .respond_with(ResponseTemplate::new(503).set_body_string("upstream down"))
        .mount(&helix)
        .await;

    let error = client(&helix, session)
        .get::<Page<Row>>(&url(&helix, "/rows"))
        .await
        .unwrap_err();
    assert!(error.is_transient(), "got {error:?}");
    assert!(!error.requires_reauth());
}

#[tokio::test]
async fn an_unreadable_body_is_reported_as_malformed_not_as_empty_data() {
    let id_server = MockServer::start().await;
    mount_validate(&id_server).await;
    let session = fresh_session(&id_server, "access1");
    session.restore().await.unwrap().unwrap();

    let helix = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rows"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{ not json"))
        .mount(&helix)
        .await;

    let error = client(&helix, session)
        .get::<Page<Row>>(&url(&helix, "/rows"))
        .await
        .unwrap_err();
    assert!(matches!(error, HelixError::Malformed(_)), "got {error:?}");
}
