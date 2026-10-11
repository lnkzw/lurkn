//! Tauri commands. Thin by design: validate, delegate to a crate, return.
//! No business logic here — see `docs/architecture.md` §8.

pub mod auth;
pub mod chat;
pub mod follows;
pub mod moderation;
pub mod overlay;
pub mod settings;
pub mod update;
pub mod whispers;
pub mod window;

use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    /// Scopes this build will request at login. Shown in Settings so the user can
    /// see exactly what was granted.
    pub scopes: Vec<String>,
    /// False until a Twitch client id is compiled in or supplied via
    /// `LURKN_CLIENT_ID`. See `docs/architecture.md` §3.5.
    pub client_id_configured: bool,
}

#[tauri::command]
#[specta::specta]
pub fn app_info() -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        scopes: lurkn_auth::REQUIRED_SCOPES
            .iter()
            .map(|s| (*s).to_owned())
            .collect(),
        client_id_configured: !lurkn_auth::client_id().is_empty(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_info_reports_the_reviewed_scope_set() {
        let info = app_info();
        assert_eq!(
            info.scopes,
            ["user:read:follows", "user:read:chat", "user:write:chat"]
        );
        assert!(!info.version.is_empty());
    }
}
