//! Device-local connection persistence. Callers hold managed_agents_store_lock.
pub(crate) mod credentials;
pub(crate) mod directory;
pub(crate) mod launch;
pub(crate) mod operations;
pub(crate) mod probe;
use buzz_connections::{model::Connection, registry::Registry};
pub(crate) use credentials::{cleanup_credentials, save_credentials, SavedCredentials};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, io::Read, path::PathBuf};
use tauri::AppHandle;

/// Secret references are private store metadata, not IPC or portable persona data.
#[derive(Default, Serialize, Deserialize)]
pub(crate) struct Store {
    #[serde(default)]
    pub registry: Registry,
    #[serde(default)]
    pub credentials: BTreeMap<String, String>,
    #[serde(default)]
    pub pending_secret_cleanup: Vec<String>,
    #[serde(default)]
    pub launches: BTreeMap<String, launch::Attempt>,
}

fn path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(crate::managed_agents::managed_agents_base_dir(app)?.join("execution-connections.json"))
}

pub(crate) fn load(app: &AppHandle) -> Result<Store, String> {
    let file = match std::fs::File::open(path(app)?) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Store::default()),
        Err(error) => return Err(format!("Cannot read connections: {error}")),
    };
    let mut data = Vec::new();
    file.take(1_048_577)
        .read_to_end(&mut data)
        .map_err(|e| e.to_string())?;
    if data.len() > 1_048_576 {
        return Err("Connection store exceeds its limit".into());
    }
    let store: Store = serde_json::from_slice(&data)
        .map_err(|_| "Connection store is invalid; restore it before continuing".to_string())?;
    for connection in &store.registry.connections {
        connection.validate()?;
    }
    Ok(store)
}

pub(crate) fn persist(app: &AppHandle, store: &Store) -> Result<(), String> {
    let data = serde_json::to_vec_pretty(store).map_err(|e| e.to_string())?;
    if data.len() > 1_048_576 {
        return Err("Connection store exceeds its limit".into());
    }
    crate::managed_agents::storage::atomic_write_json_restricted(&path(app)?, &data)
}

pub(crate) fn get(app: &AppHandle, id: &str) -> Result<Connection, String> {
    load(app)?
        .registry
        .connections
        .into_iter()
        .find(|connection| connection.id == id)
        .ok_or_else(|| "Connection no longer exists".into())
}

pub(crate) fn credential_scope(owner: &str, connection_id: &str) -> String {
    format!("{owner}:{connection_id}")
}

impl Store {
    /// A stopped receipt in one community cannot hide an active or uncertain launch elsewhere.
    pub(crate) fn has_unsettled_launch(&self, pubkey: &str) -> bool {
        self.launches.values().any(|attempt| {
            attempt.scope.agent_pubkey == pubkey
                && attempt.receipt.as_ref().is_none_or(|receipt| {
                    !matches!(
                        receipt.state,
                        buzz_connections::wire::LaunchState::Stopped
                            | buzz_connections::wire::LaunchState::Failed
                    )
                })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn terminal_scope_does_not_hide_another_live_scope() {
        let mut store = Store::default();
        let attempt = |relay: &str, state: &str| {
            serde_json::from_value(serde_json::json!({
            "scope":{"server_id":"server","owner_pubkey":"owner","relay_url":relay,"agent_pubkey":"agent"},
            "request_id":"request", "execution":{"connection_id":"connection","harness_id":"codex","model":null,"directory":{"mode":"automatic"}},
            "receipt":{"request_id":"request","scope":{"server_id":"server","owner_pubkey":"owner","relay_url":relay,"agent_pubkey":"agent"},"generation":"generation","directory":"/tmp","harness_id":"codex","state":state}
        })).unwrap()
        };
        store
            .launches
            .insert("stopped".into(), attempt("wss://one.test", "stopped"));
        assert!(!store.has_unsettled_launch("agent"));
        store
            .launches
            .insert("running".into(), attempt("wss://two.test", "running"));
        assert!(store.has_unsettled_launch("agent"));
        assert!(!store.has_unsettled_launch("another-agent"));
        store.launches.get_mut("running").unwrap().receipt = None;
        assert!(store.has_unsettled_launch("agent"));
        store
            .launches
            .insert("running".into(), attempt("wss://two.test", "failed"));
        assert!(!store.has_unsettled_launch("agent"));
    }
}
