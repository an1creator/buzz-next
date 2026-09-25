//! Journal secret-reference changes before touching the OS keyring.
use super::{persist, Store};
use serde::Deserialize;
use tauri::AppHandle;
use zeroize::Zeroize;

/// Deserialize-only input. It is never logged or written to ordinary config.
#[derive(Deserialize)]
pub(crate) struct SavedCredentials {
    pub password: String,
    pub passphrase: String,
    pub remember: bool,
}
impl Drop for SavedCredentials {
    fn drop(&mut self) {
        self.password.zeroize();
        self.passphrase.zeroize();
    }
}

fn keyring() -> Result<&'static crate::secret_store::SecretStore, String> {
    if !cfg!(feature = "system-keyring") {
        return Err("Secure storage is unavailable. Enter credentials at launch instead.".into());
    }
    Ok(crate::secret_store::SecretStore::shared(
        crate::app_state::keyring_service(),
    ))
}

/// New references are cleanup candidates until the caller commits their binding.
pub(crate) fn save_credentials(
    app: &AppHandle,
    store: &mut Store,
    owner: &str,
    value: &SavedCredentials,
) -> Result<Option<String>, String> {
    if !value.remember {
        return Ok(None);
    }
    if value.password.len() > 4096 || value.passphrase.len() > 4096 {
        return Err("Credential exceeds its limit".into());
    }
    let key = format!("ssh:{owner}:{}", uuid::Uuid::new_v4());
    let secrets = keyring()?;
    store.pending_secret_cleanup.push(key.clone());
    persist(app, store)?;
    let encoded = zeroize::Zeroizing::new(
        serde_json::to_string(&(value.password.as_str(), value.passphrase.as_str()))
            .map_err(|e| e.to_string())?,
    );
    secrets.store(&key, &encoded)?;
    Ok(Some(key))
}

/// Failed cleanup remains in the journal for the next transaction.
pub(crate) fn cleanup_credentials(app: &AppHandle, store: &mut Store) -> Result<(), String> {
    if store.pending_secret_cleanup.is_empty() {
        return Ok(());
    }
    let secrets = keyring()?;
    for key in store.pending_secret_cleanup.clone() {
        if store.credentials.values().any(|bound| bound == &key) {
            return Err("Invalid credential cleanup reference".into());
        }
        secrets.delete(&key)?;
        store
            .pending_secret_cleanup
            .retain(|pending| pending != &key);
        persist(app, store)?;
    }
    Ok(())
}

/// Load only the current owner's reference; unavailable secure storage never falls back to disk.
pub(crate) fn answers(
    store: &Store,
    owner: &str,
    connection_id: &str,
) -> Result<buzz_connections::prompt::Answers, String> {
    let mut answers = buzz_connections::prompt::Answers::default();
    if let Some(reference) = store
        .credentials
        .get(&super::credential_scope(owner, connection_id))
    {
        let encoded = keyring()?
            .load(reference)?
            .ok_or("Saved SSH credentials are unavailable. Enter them again.")?;
        let encoded = zeroize::Zeroizing::new(encoded);
        let (password, passphrase): (String, String) = serde_json::from_str(&encoded)
            .map_err(|_| "Saved SSH credentials are invalid".to_string())?;
        answers.password = zeroize::Zeroizing::new(password);
        answers.passphrase = zeroize::Zeroizing::new(passphrase);
    }
    Ok(answers)
}
