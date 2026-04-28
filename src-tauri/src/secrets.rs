use keyring::Entry;

const SERVICE: &str = "Travis";

fn entry(provider: &str) -> Result<Entry, keyring::Error> {
    Entry::new(SERVICE, &format!("{provider}_api_key"))
}

pub fn store_api_key(provider: &str, key: &str) -> Result<(), keyring::Error> {
    let result = entry(provider)?.set_password(key);
    match &result {
        Ok(()) => tracing::info!("stored api key for {provider} ({} chars)", key.len()),
        Err(e) => tracing::error!("failed to store api key for {provider}: {e}"),
    }
    result
}

pub fn get_api_key(provider: &str) -> Option<String> {
    match entry(provider).and_then(|e| e.get_password()) {
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
    }
}

#[allow(dead_code)]
pub fn delete_api_key(provider: &str) -> Result<(), keyring::Error> {
    match entry(provider)?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e),
    }
}

pub fn has_api_key(provider: &str) -> bool {
    matches!(entry(provider).and_then(|e| e.get_password()), Ok(_))
}
