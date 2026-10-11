use std::path::PathBuf;
use std::sync::Mutex;

use crate::error::{AuthError, Result};
use crate::tokens::TokenSet;

const KEYRING_SERVICE: &str = "lurkn";
const KEYRING_ENTRY: &str = "twitch-session";

/// Where the saved session lives. Reported in Settings so the user is never
/// silently downgraded to the weaker option.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageKind {
    /// OS keyring: Secret Service / kwallet on Linux, Keychain, Credential Manager.
    Keyring,
    /// Mode-0600 file. Used only where no keyring is available — a real case on
    /// minimal Wayland setups with no Secret Service running.
    File,
    /// Tests only; never persisted.
    Memory,
}

impl StorageKind {
    pub fn describe(&self) -> &'static str {
        match self {
            Self::Keyring => "your system keyring",
            Self::File => "a protected file in your home directory",
            Self::Memory => "memory (not saved)",
        }
    }
}

pub trait TokenStorage: Send + Sync + std::fmt::Debug {
    fn load(&self) -> Result<Option<TokenSet>>;
    fn store(&self, tokens: &TokenSet) -> Result<()>;
    fn clear(&self) -> Result<()>;
    fn kind(&self) -> StorageKind;
}

#[derive(Debug, Default)]
pub struct MemoryStorage {
    inner: Mutex<Option<TokenSet>>,
}

impl TokenStorage for MemoryStorage {
    fn load(&self) -> Result<Option<TokenSet>> {
        Ok(self.inner.lock().map_err(|_| poisoned())?.clone())
    }

    fn store(&self, tokens: &TokenSet) -> Result<()> {
        *self.inner.lock().map_err(|_| poisoned())? = Some(tokens.clone());
        Ok(())
    }

    fn clear(&self) -> Result<()> {
        *self.inner.lock().map_err(|_| poisoned())? = None;
        Ok(())
    }

    fn kind(&self) -> StorageKind {
        StorageKind::Memory
    }
}

fn poisoned() -> AuthError {
    AuthError::Storage("session lock was poisoned".into())
}

#[derive(Debug)]
pub struct KeyringStorage;

impl KeyringStorage {
    fn entry() -> Result<keyring::Entry> {
        keyring::Entry::new(KEYRING_SERVICE, KEYRING_ENTRY)
            .map_err(|e| AuthError::Storage(e.to_string()))
    }

    /// Probe whether a usable keyring backend exists on this machine.
    pub fn is_available() -> bool {
        Self::entry().is_ok_and(|entry| {
            !matches!(
                entry.get_password(),
                Err(keyring::Error::PlatformFailure(_))
            )
        })
    }
}

impl TokenStorage for KeyringStorage {
    fn load(&self) -> Result<Option<TokenSet>> {
        match Self::entry()?.get_password() {
            Ok(json) => serde_json::from_str(&json)
                .map(Some)
                // A session we cannot parse is a session we cannot use. Treat it as
                // absent rather than failing startup.
                .or_else(|e| {
                    tracing::warn!(error = %e, "discarding unreadable saved session");
                    Ok(None)
                }),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(AuthError::Storage(e.to_string())),
        }
    }

    fn store(&self, tokens: &TokenSet) -> Result<()> {
        let json = serde_json::to_string(tokens)
            .map_err(|e| AuthError::Storage(format!("could not serialize session: {e}")))?;
        Self::entry()?
            .set_password(&json)
            .map_err(|e| AuthError::Storage(e.to_string()))
    }

    fn clear(&self) -> Result<()> {
        match Self::entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(AuthError::Storage(e.to_string())),
        }
    }

    fn kind(&self) -> StorageKind {
        StorageKind::Keyring
    }
}

#[derive(Debug)]
pub struct FileStorage {
    path: PathBuf,
}

impl FileStorage {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// `session.json` beside the cache, in the platform's local data folder
    /// (see `Store::default_path` in lurkn-store).
    pub fn default_path() -> Option<PathBuf> {
        dirs::data_local_dir().map(|base| base.join("lurkn").join("session.json"))
    }

    #[cfg(unix)]
    fn harden(path: &std::path::Path) -> Result<()> {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| AuthError::Storage(format!("could not restrict {}: {e}", path.display())))
    }

    #[cfg(not(unix))]
    fn harden(_path: &std::path::Path) -> Result<()> {
        Ok(())
    }
}

impl TokenStorage for FileStorage {
    fn load(&self) -> Result<Option<TokenSet>> {
        match std::fs::read_to_string(&self.path) {
            Ok(json) => serde_json::from_str(&json).map(Some).or_else(|e| {
                tracing::warn!(error = %e, "discarding unreadable saved session");
                Ok(None)
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(AuthError::Storage(e.to_string())),
        }
    }

    fn store(&self, tokens: &TokenSet) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                AuthError::Storage(format!("could not create {}: {e}", parent.display()))
            })?;
        }
        let json = serde_json::to_string(tokens)
            .map_err(|e| AuthError::Storage(format!("could not serialize session: {e}")))?;

        // Write, then tighten permissions before anything else can open it.
        std::fs::write(&self.path, json).map_err(|e| AuthError::Storage(e.to_string()))?;
        Self::harden(&self.path)
    }

    fn clear(&self) -> Result<()> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(AuthError::Storage(e.to_string())),
        }
    }

    fn kind(&self) -> StorageKind {
        StorageKind::File
    }
}

/// Prefer the keyring; fall back to a protected file, loudly.
pub fn default_storage() -> Box<dyn TokenStorage> {
    if KeyringStorage::is_available() {
        return Box::new(KeyringStorage);
    }

    match FileStorage::default_path() {
        Some(path) => {
            tracing::warn!(
                path = %path.display(),
                "no system keyring available; saving the Twitch session to a 0600 file instead"
            );
            Box::new(FileStorage::new(path))
        }
        None => {
            tracing::warn!("no keyring and no home directory; the session will not be saved");
            Box::new(MemoryStorage::default())
        }
    }
}

#[cfg(test)]
mod tests {
    use time::{Duration, OffsetDateTime};

    use super::*;

    fn token(access: &str) -> TokenSet {
        TokenSet {
            access_token: access.to_owned(),
            refresh_token: "refresh".to_owned(),
            obtained_at: OffsetDateTime::now_utc(),
            expires_in: Duration::seconds(14820),
            scopes: crate::scopes::REQUIRED_SCOPES
                .iter()
                .map(|s| (*s).to_owned())
                .collect(),
        }
    }

    #[test]
    fn memory_storage_round_trips() {
        let storage = MemoryStorage::default();
        assert!(storage.load().unwrap().is_none());

        storage.store(&token("first")).unwrap();
        assert_eq!(storage.load().unwrap().unwrap().access_token, "first");

        storage.store(&token("second")).unwrap();
        assert_eq!(storage.load().unwrap().unwrap().access_token, "second");

        storage.clear().unwrap();
        assert!(storage.load().unwrap().is_none());
    }

    #[test]
    fn file_storage_round_trips_and_is_owner_only() {
        let dir = std::env::temp_dir().join(format!("lurkn-test-{}", std::process::id()));
        let path = dir.join("session.json");
        let storage = FileStorage::new(path.clone());

        assert!(
            storage.load().unwrap().is_none(),
            "missing file reads as no session"
        );

        storage.store(&token("saved")).unwrap();
        assert_eq!(storage.load().unwrap().unwrap().access_token, "saved");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(
                mode, 0o600,
                "session file must not be readable by other users"
            );
        }

        storage.clear().unwrap();
        assert!(storage.load().unwrap().is_none());
        storage.clear().unwrap();

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_corrupt_session_file_reads_as_no_session() {
        let dir = std::env::temp_dir().join(format!("lurkn-corrupt-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("session.json");
        std::fs::write(&path, "{ not json").unwrap();

        // Startup must not fail; the user simply gets the login screen.
        assert!(FileStorage::new(path).load().unwrap().is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn storage_kinds_describe_themselves_for_the_settings_screen() {
        assert_eq!(StorageKind::Keyring.describe(), "your system keyring");
        assert_eq!(
            StorageKind::File.describe(),
            "a protected file in your home directory"
        );
        assert_ne!(
            StorageKind::Keyring.describe(),
            StorageKind::File.describe()
        );
    }
}
