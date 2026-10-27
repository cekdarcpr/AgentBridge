use std::env;

use keyring::Entry;
use serde::Serialize;

use crate::{capability::ProviderId, error::AppError};

const SERVICE: &str = "agentbridge";
/// Skips the OS keychain entirely (CI, containers, tests); keys then come only from env vars.
pub const NO_KEYCHAIN_ENV: &str = "AGENTBRIDGE_NO_KEYCHAIN";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum KeySource {
    Keychain,
    Env,
}

pub struct ApiKey {
    pub value: String,
    pub source: KeySource,
}

// Deliberately not Debug-printing the value anywhere.
impl std::fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApiKey")
            .field("source", &self.source)
            .finish_non_exhaustive()
    }
}

fn keychain_enabled() -> bool {
    env::var_os(NO_KEYCHAIN_ENV).is_none_or(|v| v.is_empty() || v == "0")
}

fn entry(provider: ProviderId) -> Result<Entry, AppError> {
    Entry::new(SERVICE, provider.as_str())
        .map_err(|e| AppError::Keychain(format!("keychain unavailable: {e}")))
}

/// Keychain wins over env vars so a deliberately stored key is never shadowed by a stale shell variable.
pub fn get(provider: ProviderId) -> Result<Option<ApiKey>, AppError> {
    if keychain_enabled() {
        // A broken keychain (e.g. headless Linux) should not block env-var keys.
        if let Ok(value) = entry(provider).and_then(|e| {
            e.get_password()
                .map_err(|e| AppError::Keychain(e.to_string()))
        }) {
            if !value.trim().is_empty() {
                return Ok(Some(ApiKey {
                    value: value.trim().to_string(),
                    source: KeySource::Keychain,
                }));
            }
        }
    }
    match env::var(provider.env_var()) {
        Ok(value) if !value.trim().is_empty() => Ok(Some(ApiKey {
            value: value.trim().to_string(),
            source: KeySource::Env,
        })),
        _ => Ok(None),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum KeyStatus {
    Keychain,
    Env,
    Missing,
}

impl KeyStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Keychain => "keychain",
            Self::Env => "env",
            Self::Missing => "missing",
        }
    }
}

pub fn status(provider: ProviderId) -> KeyStatus {
    match get(provider) {
        Ok(Some(key)) if key.source == KeySource::Keychain => KeyStatus::Keychain,
        Ok(Some(_)) => KeyStatus::Env,
        _ => KeyStatus::Missing,
    }
}

pub fn set(provider: ProviderId, value: &str) -> Result<(), AppError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(AppError::Usage("API key is empty".into()));
    }
    if !keychain_enabled() {
        return Err(AppError::Keychain(format!(
            "keychain disabled by {NO_KEYCHAIN_ENV}"
        )));
    }
    entry(provider)?
        .set_password(value)
        .map_err(|e| AppError::Keychain(format!("could not store key: {e}")))
}

/// Returns whether a key was present.
pub fn remove(provider: ProviderId) -> Result<bool, AppError> {
    match entry(provider)?.delete_credential() {
        Ok(()) => Ok(true),
        Err(keyring::Error::NoEntry) => Ok(false),
        Err(e) => Err(AppError::Keychain(format!("could not remove key: {e}"))),
    }
}
