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

pub fn get_api_key(provider: &str) -> Option<String> {
    // Fast path: cached in process memory. No OS keychain hit, no
    // password prompt, no IPC. This is the common case after the
    // first successful read of the session.
    if let Ok(g) = cache().lock() {
        if let Some(cached) = g.get(provider) {
            return Some(cached.clone());
        }
    }

    // Slow path: cold cache or first call. Hits the keychain (and
    // on macOS may surface the user-password modal once).
    let fetched = match entry(provider).and_then(|e| e.get_password()) {
        Ok(s) if !s.is_empty() => Some(s),
        Ok(_) => {
            tracing::warn!("keyring entry for {provider} was empty");
            None
        }
        Err(keyring::Error::NoEntry) => None,
        Err(e) => {
            tracing::warn!("keyring get_password failed for {provider}: {e}");
            None
        }
    };

    if let Some(key) = fetched.as_ref() {
        if let Ok(mut g) = cache().lock() {
            g.insert(provider.to_string(), key.clone());
        }
    }
    fetched
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
