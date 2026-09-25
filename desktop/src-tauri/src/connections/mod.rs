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
