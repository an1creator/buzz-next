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
    // A failed Remember attempt may leave a journal entry while the keyring is
    // unavailable. Preserve it for retry without blocking a non-Remember save.
    let secrets = match keyring() {
        Ok(secrets) => secrets,
        Err(_) => return Ok(()),
    };
    drain_cleanup(
        store,
        |key| secrets.delete(key),
        |store| persist(app, store),
    )
}

fn drain_cleanup(
    store: &mut Store,
    mut delete: impl FnMut(&str) -> Result<(), String>,
    mut persist_store: impl FnMut(&Store) -> Result<(), String>,
) -> Result<(), String> {
    for key in store.pending_secret_cleanup.clone() {
        if store.credentials.values().any(|bound| bound == &key) {
            return Err("Invalid credential cleanup reference".into());
        }
        validate_reference(&key, None)?;
        if delete(&key).is_err() {
            // Do not discard an orphan reference until its keyring entry is gone.
            break;
        }
        store
            .pending_secret_cleanup
            .retain(|pending| pending != &key);
        persist_store(store)?;
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
        validate_reference(reference, Some(owner))?;
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

/// Restrict cleanup and reads to this feature's credential namespace.
pub(crate) fn validate_reference(reference: &str, owner: Option<&str>) -> Result<(), String> {
    let mut parts = reference.split(':');
    let prefix = parts.next();
    let identity = parts.next().unwrap_or_default();
    let id = parts.next().unwrap_or_default();
    if prefix != Some("ssh")
        || identity.len() != 64
        || !identity.bytes().all(|c| c.is_ascii_hexdigit())
        || owner.is_some_and(|owner| owner != identity)
        || uuid::Uuid::parse_str(id).is_err()
        || parts.next().is_some()
    {
        return Err("Invalid SSH credential reference".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{drain_cleanup, validate_reference};
    use crate::connections::Store;
    #[test]
    fn references_cannot_read_other_owners_or_delete_unrelated_secrets() {
        let owner = "a".repeat(64);
        let reference = format!("ssh:{owner}:{}", uuid::Uuid::new_v4());
        assert!(validate_reference(&reference, Some(&owner)).is_ok());
        assert!(validate_reference(&reference, Some(&"b".repeat(64))).is_err());
        assert!(validate_reference("identity-private-key", None).is_err());
        assert!(validate_reference("ssh:invalid:invalid", None).is_err());
    }

    #[test]
    fn failed_keyring_cleanup_keeps_its_journal_without_blocking_later_saves() {
        let reference = format!("ssh:{}:{}", "a".repeat(64), uuid::Uuid::new_v4());
        let mut store = Store {
            pending_secret_cleanup: vec![reference.clone()],
            ..Store::default()
        };
        let mut writes = 0;
        drain_cleanup(
            &mut store,
            |_| Err("keyring unavailable".into()),
            |_| {
                writes += 1;
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(store.pending_secret_cleanup, [reference]);
        assert_eq!(writes, 0);

        drain_cleanup(
            &mut store,
            |_| Ok(()),
            |_| {
                writes += 1;
                Ok(())
            },
        )
        .unwrap();
        assert!(store.pending_secret_cleanup.is_empty());
        assert_eq!(writes, 1);
    }
}
