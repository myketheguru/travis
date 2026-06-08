//! API-key storage backed by the OS keychain, with an in-process
//! cache so we only prompt the user for keychain unlock once per
//! launch.
//!
//! On macOS, every keychain access to an item that isn't "Always
//! Allow" pops the user-password modal. Without caching, every LLM
//! request (journal capture, memory query, proactive nudge,
//! summary, etc.) triggers a prompt. With the cache below, the
//! prompt fires at most once per process for each provider —
//! subsequent reads hit memory.
//!
//! Threat model: the secret is in process memory whenever it's used
//! to authenticate an HTTP call. Holding it longer doesn't change
//! the attacker model meaningfully; if a hostile process can read
//! ours, the secret was already exposed during any active request.
//! The cache is cleared on process exit and on explicit
//! `delete_api_key`.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use keyring::Entry;

const SERVICE: &str = "Travis";

fn entry(provider: &str) -> Result<Entry, keyring::Error> {
    Entry::new(SERVICE, &format!("{provider}_api_key"))
}

/// Process-wide cache of (provider -> api_key). Populated on first
/// successful keychain read; updated on store; evicted on delete.
fn cache() -> &'static Mutex<HashMap<String, String>> {
    static CACHE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn store_api_key(provider: &str, key: &str) -> Result<(), keyring::Error> {
    let result = entry(provider)?.set_password(key);
    match &result {
        Ok(()) => {
            tracing::info!("stored api key for {provider} ({} chars)", key.len());
            // Mirror into the cache so subsequent get_api_key calls
            // don't have to round-trip back through the keychain.
            if let Ok(mut g) = cache().lock() {
                g.insert(provider.to_string(), key.to_string());
            }
        }
        Err(e) => tracing::error!("failed to store api key for {provider}: {e}"),
    }
    result
}

/// Detailed lookup outcome — used both for ergonomic Option returns
/// AND for surfacing accurate error messages when the key is missing
/// so the user can tell whether it's "no key stored" vs "OS keychain
/// returned an error" vs "key stored but empty".
#[derive(Debug, Clone)]
pub enum KeyLookup {
    /// Cache hit — fast path.
    FromCache(String),
    /// Cold cache, keychain returned the key.
    FromKeychain(String),
    /// No entry stored. User probably never finished onboarding for
    /// this provider, or the storage was reset.
    NoEntry,
    /// Keychain has an entry but it's empty.
    EmptyEntry,
    /// Keychain returned an unexpected error (permissions, encoding,
    /// platform glitch). The string is the underlying error message
    /// so we can surface it to the user instead of saying "key not
    /// found" when really the OS keychain is misbehaving.
    KeychainError(String),
}

impl KeyLookup {
    pub fn as_option(self) -> Option<String> {
        match self {
            KeyLookup::FromCache(k) | KeyLookup::FromKeychain(k) => Some(k),
            KeyLookup::NoEntry | KeyLookup::EmptyEntry | KeyLookup::KeychainError(_) => None,
        }
    }
}

/// Detailed lookup primarily used by error-surfacing call sites
/// (`llm::build`, the test-provider command). Most callers want the
/// `Option<String>` convenience wrapper `get_api_key`.
pub fn lookup_api_key(provider: &str) -> KeyLookup {
    // Fast path: cached in process memory. No OS keychain hit, no
    // password prompt, no IPC. Common case after the first
    // successful read of the session.
    if let Ok(g) = cache().lock() {
        if let Some(cached) = g.get(provider) {
            return KeyLookup::FromCache(cached.clone());
        }
    }

    // Slow path: cold cache or first call. Hits the keychain.
    match entry(provider).and_then(|e| e.get_password()) {
        Ok(s) if !s.is_empty() => {
            tracing::info!(
                "secrets: keychain read OK for {provider} ({} chars)",
                s.len()
            );
            if let Ok(mut g) = cache().lock() {
                g.insert(provider.to_string(), s.clone());
            }
            KeyLookup::FromKeychain(s)
        }
        Ok(_) => {
            tracing::warn!("secrets: keychain entry for {provider} was empty");
            KeyLookup::EmptyEntry
        }
        Err(keyring::Error::NoEntry) => {
            tracing::info!("secrets: no keychain entry for {provider}");
            KeyLookup::NoEntry
        }
        Err(e) => {
            let msg = e.to_string();
            tracing::warn!("secrets: keychain get_password failed for {provider}: {msg}");
            KeyLookup::KeychainError(msg)
        }
    }
}

pub fn get_api_key(provider: &str) -> Option<String> {
    lookup_api_key(provider).as_option()
}

#[allow(dead_code)]
pub fn delete_api_key(provider: &str) -> Result<(), keyring::Error> {
    // Evict the cache regardless of whether the keychain delete
    // succeeds — we don't want stale in-memory secrets surviving an
    // intended removal.
    if let Ok(mut g) = cache().lock() {
        g.remove(provider);
    }
    match entry(provider)?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e),
    }
}

pub fn has_api_key(provider: &str) -> bool {
    // Cheap: cache lookup first, then keychain probe (which may
    // prompt on cold cache — but only the first time per launch).
    if let Ok(g) = cache().lock() {
        if g.contains_key(provider) {
            return true;
        }
    }
    matches!(entry(provider).and_then(|e| e.get_password()), Ok(_))
}
