#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! End-to-end device-code and refresh flows against a mock identity server.
//!
//! Response shapes mirror what live Twitch returned on 2026-09-20, including the
//! non-RFC error envelope `{"status":400,"message":"authorization_pending"}`.

use lurkn_auth::error::AuthError;
use lurkn_auth::storage::{MemoryStorage, TokenStorage};
use lurkn_auth::{IdEndpoints, Session};
use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const CLIENT_ID: &str = "testclientid0000000000000000000";
const SCOPES: [&str; 3] = ["user:read:follows", "user:read:chat", "user:write:chat"];

fn session(server: &MockServer) -> Session {
    Session::new(
        reqwest::Client::new(),
        IdEndpoints::new(&server.uri()).unwrap(),
        CLIENT_ID.to_owned(),
        Box::new(MemoryStorage::default()),
    )
}

fn token_body(access: &str, refresh: &str) -> serde_json::Value {
    json!({
        "access_token": access,
        "refresh_token": refresh,
        "expires_in": 14820,
        "token_type": "bearer",
        "scope": SCOPES,
    })
}

fn validate_body() -> serde_json::Value {
    json!({
        "client_id": CLIENT_ID,
        "login": "testuser",
        "user_id": "12345",
        "scopes": SCOPES,
        "expires_in": 14820,
    })
}

fn twitch_error(status: u16, message: &str) -> ResponseTemplate {
    ResponseTemplate::new(status).set_body_json(json!({ "status": status, "message": message }))
}

async fn mock_device(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/oauth2/device"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "device_code": "DEVICECODE",
            "expires_in": 1800,
            "interval": 5,
            "user_code": "RZQXXTVC",
            "verification_uri": "https://www.twitch.tv/activate?device-code=RZQXXTVC",
        })))
        .mount(server)
        .await;
}

#[tokio::test]
async fn begin_login_returns_a_code_to_show_the_user() {
    let server = MockServer::start().await;
    mock_device(&server).await;

    let prompt = session(&server).begin_login().await.unwrap();

    assert_eq!(prompt.user_code, "RZQXXTVC");
    assert_eq!(prompt.device_code, "DEVICECODE");
    assert_eq!(prompt.interval, time::Duration::seconds(5));
    assert!(prompt.expires_at > time::OffsetDateTime::now_utc());
    assert_eq!(prompt.verification_uri.host_str(), Some("www.twitch.tv"));
}

#[tokio::test]
async fn polling_reports_pending_then_succeeds() {
    let server = MockServer::start().await;
    mock_device(&server).await;

    // First poll: still waiting on the user.
    Mock::given(method("POST"))
        .and(path("/oauth2/token"))
        .respond_with(twitch_error(400, "authorization_pending"))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/oauth2/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(token_body("access1", "refresh1")))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/oauth2/validate"))
        .and(header("Authorization", "OAuth access1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(validate_body()))
        .mount(&server)
        .await;

    let session = session(&server);
    let prompt = session.begin_login().await.unwrap();

    let pending = session
        .complete_login(&prompt.device_code)
        .await
        .unwrap_err();
    assert_eq!(pending, AuthError::AuthorizationPending);
    assert!(pending.is_pending(), "the caller must keep polling");

    let identity = session.complete_login(&prompt.device_code).await.unwrap();
    assert_eq!(identity.login.as_str(), "testuser");
    assert_eq!(identity.user_id.as_str(), "12345");

    // The access token is now usable without another network call.
    assert_eq!(session.access_token().await.unwrap(), "access1");
}

#[tokio::test]
async fn a_declined_login_is_terminal() {
    let server = MockServer::start().await;
    mock_device(&server).await;
    Mock::given(method("POST"))
        .and(path("/oauth2/token"))
        .respond_with(twitch_error(400, "user denied the request"))
        .mount(&server)
        .await;

    let err = session(&server)
        .complete_login("DEVICECODE")
        .await
        .unwrap_err();
    assert_eq!(err, AuthError::Denied);
    assert!(!err.is_pending());
    assert!(err.requires_reauth());
}

#[tokio::test]
async fn an_expired_device_code_is_terminal() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oauth2/token"))
        .respond_with(twitch_error(400, "invalid device code"))
        .mount(&server)
        .await;

    let err = session(&server).complete_login("STALE").await.unwrap_err();
    assert_eq!(err, AuthError::DeviceCodeExpired);
}

/// Twitch accepts both `scope` and `scopes` on the device endpoint, so a token
/// can come back granting nothing. That must fail at login, not later.
#[tokio::test]
async fn a_token_missing_scopes_is_refused_and_not_stored() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oauth2/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "unscoped",
            "refresh_token": "r",
            "expires_in": 14820,
            "scope": ["user:read:follows"],
        })))
        .mount(&server)
        .await;

    let session = session(&server);
    let err = session.complete_login("DEVICECODE").await.unwrap_err();
    assert_eq!(
        err,
        AuthError::ScopesNotGranted {
            missing: vec!["user:read:chat".into(), "user:write:chat".into()]
        }
    );

    // Nothing was persisted, so the app stays logged out.
    assert!(matches!(
        session.access_token().await,
        Err(AuthError::RefreshRejected)
    ));
}

#[tokio::test]
async fn restore_refreshes_a_stale_token_and_stores_the_rotated_one() {
    let server = MockServer::start().await;

    // Twitch rotates the refresh token on every refresh.
    Mock::given(method("POST"))
        .and(path("/oauth2/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(token_body("access2", "refresh2")))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/oauth2/validate"))
        .respond_with(ResponseTemplate::new(200).set_body_json(validate_body()))
        .mount(&server)
        .await;

    let storage = MemoryStorage::default();
    storage
        .store(&lurkn_auth::TokenSet {
            access_token: "stale".into(),
            refresh_token: "refresh1".into(),
            // Obtained five hours ago: past expiry, well inside the 30-day window.
            obtained_at: time::OffsetDateTime::now_utc() - time::Duration::hours(5),
            expires_in: time::Duration::seconds(14820),
            scopes: SCOPES.iter().map(|s| (*s).to_owned()).collect(),
        })
        .unwrap();

    let session = Session::new(
        reqwest::Client::new(),
        IdEndpoints::new(&server.uri()).unwrap(),
        CLIENT_ID.to_owned(),
        Box::new(storage),
    );

    let identity = session
        .restore()
        .await
        .unwrap()
        .expect("a session was saved");
    assert_eq!(identity.login.as_str(), "testuser");
    assert_eq!(session.access_token().await.unwrap(), "access2");
}

#[tokio::test]
async fn restore_finds_nothing_when_no_session_was_saved() {
    let server = MockServer::start().await;
    assert!(session(&server).restore().await.unwrap().is_none());
}

/// Past the 30-day public-client ceiling there is nothing to refresh, so the
/// network is not even touched.
#[tokio::test]
async fn restore_past_the_refresh_window_demands_a_new_login() {
    let server = MockServer::start().await;

    let storage = MemoryStorage::default();
    storage
        .store(&lurkn_auth::TokenSet {
            access_token: "ancient".into(),
            refresh_token: "ancient".into(),
            obtained_at: time::OffsetDateTime::now_utc() - time::Duration::days(31),
            expires_in: time::Duration::seconds(14820),
            scopes: SCOPES.iter().map(|s| (*s).to_owned()).collect(),
        })
        .unwrap();

    let session = Session::new(
        reqwest::Client::new(),
        IdEndpoints::new(&server.uri()).unwrap(),
        CLIENT_ID.to_owned(),
        Box::new(storage),
    );

    let err = session.restore().await.unwrap_err();
    assert_eq!(err, AuthError::RefreshRejected);
    assert!(err.requires_reauth());
    assert_eq!(
        server.received_requests().await.unwrap().len(),
        0,
        "no request should be sent"
    );
}

#[tokio::test]
async fn a_revoked_refresh_token_demands_a_new_login() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oauth2/token"))
        .respond_with(twitch_error(400, "Invalid refresh token"))
        .mount(&server)
        .await;

    let storage = MemoryStorage::default();
    storage
        .store(&lurkn_auth::TokenSet {
            access_token: "stale".into(),
            refresh_token: "revoked".into(),
            obtained_at: time::OffsetDateTime::now_utc() - time::Duration::hours(5),
            expires_in: time::Duration::seconds(14820),
            scopes: SCOPES.iter().map(|s| (*s).to_owned()).collect(),
        })
        .unwrap();

    let session = Session::new(
        reqwest::Client::new(),
        IdEndpoints::new(&server.uri()).unwrap(),
        CLIENT_ID.to_owned(),
        Box::new(storage),
    );

    assert_eq!(
        session.restore().await.unwrap_err(),
        AuthError::RefreshRejected
    );
}

/// A token Twitch no longer recognises (revoked from the connections page) must
/// not present as logged in.
#[tokio::test]
async fn a_token_rejected_by_validate_is_not_adopted() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/oauth2/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(token_body("access1", "refresh1")))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/oauth2/validate"))
        .respond_with(twitch_error(401, "invalid access token"))
        .mount(&server)
        .await;

    let session = session(&server);
    let err = session.complete_login("DEVICECODE").await.unwrap_err();
    assert_eq!(
        err,
        AuthError::Twitch {
            status: 401,
            message: "invalid access token".into()
        }
    );
    assert!(matches!(
        session.access_token().await,
        Err(AuthError::RefreshRejected)
    ));
}

#[tokio::test]
async fn logout_forgets_the_session() {
    let server = MockServer::start().await;
    mock_device(&server).await;
    Mock::given(method("POST"))
        .and(path("/oauth2/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(token_body("access1", "refresh1")))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/oauth2/validate"))
        .respond_with(ResponseTemplate::new(200).set_body_json(validate_body()))
        .mount(&server)
        .await;

    let session = session(&server);
    session.complete_login("DEVICECODE").await.unwrap();
    assert!(session.access_token().await.is_ok());

    session.logout().await.unwrap();
    assert!(matches!(
        session.access_token().await,
        Err(AuthError::RefreshRejected)
    ));
}

/// The device code is a credential: it must never reach a user-facing string.
#[tokio::test]
async fn the_device_code_is_not_part_of_the_user_facing_prompt() {
    let server = MockServer::start().await;
    mock_device(&server).await;

    let prompt = session(&server).begin_login().await.unwrap();
    assert!(!prompt.user_code.contains(&prompt.device_code));
    assert!(!prompt
        .verification_uri
        .as_str()
        .contains(&prompt.device_code));
}
