//! Personal channel working folders. No paths are published to the relay.
use super::{BackendKind, ManagedAgentRecord};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, io::Read, path::Path};
use tauri::AppHandle;
use uuid::Uuid;

#[derive(Default, Serialize, Deserialize)]
struct Store {
    revision: u64,
    channels: BTreeMap<Uuid, String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelFolder {
    pub path: Option<String>,
    pub revision: u64,
}

/// Backend identity includes the full provider configuration (host and profile).
/// Local settings belong to this installation's app-data directory.
pub fn target_id(backend: &BackendKind) -> Result<String, String> {
    let bytes = serde_json::to_vec(backend).map_err(|e| e.to_string())?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn store_path<R: tauri::Runtime>(
    app: &AppHandle<R>,
    owner: &str,
    relay: &str,
    backend: &BackendKind,
) -> Result<std::path::PathBuf, String> {
    Ok(super::storage::managed_agents_base_dir(app)?.join(filename(owner, relay, backend)?))
}

fn filename(owner: &str, relay: &str, backend: &BackendKind) -> Result<String, String> {
    let scope = super::ManagedAgentRuntimeKey::new(owner, relay)?;
    Ok(format!(
        "working-folders-{}-{}.json",
        scope.runtime_id(),
        target_id(backend)?
    ))
}

fn read(path: &Path) -> Result<Store, String> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Store::default()),
        Err(e) => return Err(format!("Cannot read working folders: {e}")),
    };
    let mut raw = Vec::new();
    file.take((buzz_workspaces::MAX_CONFIG_BYTES + 129) as u64)
        .read_to_end(&mut raw)
        .map_err(|e| format!("Cannot read working folders: {e}"))?;
    if raw.len() > buzz_workspaces::MAX_CONFIG_BYTES + 128 {
        return Err("Working folder settings exceed the size limit".into());
    }
    let store: Store = serde_json::from_slice(&raw).map_err(|_| {
        "Working folder settings are invalid; repair the saved file before continuing"
    })?;
    buzz_workspaces::Workspaces {
        default: None,
        channels: store.channels.clone(),
    }
    .validate()?;
    Ok(store)
}

pub fn channel<R: tauri::Runtime>(
    app: &AppHandle<R>,
    owner: &str,
    relay: &str,
    backend: &BackendKind,
    channel: Uuid,
) -> Result<ChannelFolder, String> {
    let store = read(&store_path(app, owner, relay, backend)?)?;
    Ok(ChannelFolder {
        path: store.channels.get(&channel).cloned(),
        revision: store.revision,
    })
}

/// Caller holds managed_agents_store_lock. A revision rejects stale editors.
pub fn save<R: tauri::Runtime>(
    app: &AppHandle<R>,
    owner: &str,
    relay: &str,
    backend: &BackendKind,
    channel: Uuid,
    path: Option<String>,
    revision: u64,
) -> Result<ChannelFolder, String> {
    let file = store_path(app, owner, relay, backend)?;
    save_file(&file, backend, channel, path, revision)
}

fn save_file(
    file: &Path,
    backend: &BackendKind,
    channel: Uuid,
    path: Option<String>,
    revision: u64,
) -> Result<ChannelFolder, String> {
    let mut store = read(file)?;
    if store.revision != revision {
        return Err("Working folders changed in another window. Reload before saving.".into());
    }
    let path = path.filter(|p| !p.trim().is_empty());
    if let Some(path) = &path {
        buzz_workspaces::validate_path_text(path)?;
        if *backend == BackendKind::Local {
            buzz_workspaces::canonical_folder(Path::new(path))?;
        }
        store.channels.insert(channel, path.clone());
    } else {
        store.channels.remove(&channel);
    }
    buzz_workspaces::Workspaces {
        default: None,
        channels: store.channels.clone(),
    }
    .validate()?;
    store.revision = store
        .revision
        .checked_add(1)
        .ok_or("Working folder revision overflow")?;
    super::storage::atomic_write_json_restricted(
        file,
        &serde_json::to_vec(&store).map_err(|e| e.to_string())?,
    )?;
    Ok(ChannelFolder {
        path,
        revision: store.revision,
    })
}

/// Capture only the launching owner/community/target, never the active UI state.
pub fn launch_channels<R: tauri::Runtime>(
    app: &AppHandle<R>,
    owner: &str,
    relay: &str,
    record: &ManagedAgentRecord,
) -> Result<String, String> {
    let store = read(&store_path(app, owner, relay, &record.backend)?)?;
    serde_json::to_string(&store.channels).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_are_scoped_by_owner_community_and_execution_target() {
        let owner = "a".repeat(64);
        let local = BackendKind::Local;
        let remote = BackendKind::Provider {
            id: "ssh".into(),
            config: serde_json::json!({"host":"server","profile":"shared-codex"}),
        };
        let a = filename(&owner, "https://one.example/", &local).unwrap();
        assert_eq!(a, filename(&owner, "wss://one.example", &local).unwrap());
        assert_ne!(
            a,
            filename(&"b".repeat(64), "wss://one.example", &local).unwrap()
        );
        assert_ne!(a, filename(&owner, "wss://two.example", &local).unwrap());
        assert_ne!(a, filename(&owner, "wss://one.example", &remote).unwrap());
    }

    #[test]
    fn channel_save_is_atomic_revision_checked_and_clear_inherits() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("folders.json");
        let channel = Uuid::new_v4();
        let other = Uuid::new_v4();
        let path = root.path().to_str().unwrap().to_string();
        save_file(&file, &BackendKind::Local, channel, Some(path.clone()), 0).unwrap();
        let before = std::fs::read(&file).unwrap();
        assert!(save_file(&file, &BackendKind::Local, other, Some(path.clone()), 0).is_err());
        assert!(save_file(
            &file,
            &BackendKind::Local,
            other,
            Some("relative".into()),
            1
        )
        .is_err());
        assert_eq!(std::fs::read(&file).unwrap(), before);
        save_file(&file, &BackendKind::Local, other, Some(path.clone()), 1).unwrap();
        save_file(&file, &BackendKind::Local, channel, None, 2).unwrap();
        let store = read(&file).unwrap();
        assert!(!store.channels.contains_key(&channel));
        assert_eq!(store.channels.get(&other), Some(&path));
    }

    #[test]
    fn malformed_saved_settings_fail_closed() {
        let root = tempfile::tempdir().unwrap();
        let file = root.path().join("folders.json");
        std::fs::write(&file, "broken").unwrap();
        assert!(read(&file).is_err());
    }
}
