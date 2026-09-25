//! Explicit connection management; discovery and launches are separate operations.
use crate::{app_state::AppState, connections};
use buzz_connections::model::Connection;
use tauri::{AppHandle, State};

/// Read saved locations. Empty means the user has not added one yet.
#[tauri::command]
pub fn list_execution_connections(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<Connection>, String> {
    let _guard = state
        .managed_agents_store_lock
        .lock()
        .map_err(|e| e.to_string())?;
    Ok(connections::load(&app)?.registry.connections)
}

/// Save one revision without installing tools or starting agents.
#[tauri::command]
pub fn save_execution_connection(
    connection: Connection,
    expected_revision: Option<u64>,
    credentials: Option<connections::SavedCredentials>,
    check_ticket: Option<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Connection, String> {
    let owner = super::agents::workspace_owner_hex(&state)?;
    let _guard = state
        .managed_agents_store_lock
        .lock()
        .map_err(|e| e.to_string())?;
    let mut store = connections::load(&app)?;
    connections::cleanup_credentials(&app, &mut store)?;
    let old = store
        .registry
        .connections
        .iter()
        .find(|item| item.id == connection.id)
        .cloned();
    let mut registry = store.registry.clone();
    let mut saved = registry.save(connection, expected_revision)?;
    if let Some(ticket) = check_ticket {
        let check = connections::probe::checked(&owner, &saved, &ticket)?;
        registry.record_check(&saved.id, check.clone())?;
        saved.check = Some(check);
    }
    let scope = connections::credential_scope(&owner, &saved.id);
    let new_reference = match &credentials {
        Some(value) => connections::save_credentials(&app, &mut store, &owner, value)?,
        None => None,
    };
    // Access changes invalidate references for every owner, not just the current identity.
    let access_changed = old.as_ref().is_some_and(|old| old.target != saved.target);
    let obsolete: Vec<_> = store
        .credentials
        .keys()
        .filter(|key| {
            (access_changed && key.ends_with(&format!(":{}", saved.id)))
                || (credentials.is_some() && **key == scope)
        })
        .cloned()
        .collect();
    for key in obsolete {
        if let Some(reference) = store.credentials.remove(&key) {
            store.pending_secret_cleanup.push(reference);
        }
    }
    if let Some(reference) = new_reference {
        store.pending_secret_cleanup.retain(|key| key != &reference);
        store.credentials.insert(scope, reference);
    }
    store.registry = registry;
    connections::persist(&app, &store)?;
    // Cleanup is journalled; don't turn a successful config save into a failed Save.
    // The next mutation retries cleanup before accepting another write.
    Ok(saved)
}

/// Remove only unused locations; deletion never controls a remote process.
#[tauri::command]
pub fn delete_execution_connection(
    id: String,
    expected_revision: u64,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let _guard = state
        .managed_agents_store_lock
        .lock()
        .map_err(|e| e.to_string())?;
    let mut store = connections::load(&app)?;
    connections::cleanup_credentials(&app, &mut store)?;
    let records = crate::managed_agents::load_managed_agents(&app)?;
    let references: Vec<_> = records
        .iter()
        .filter(|record| {
            record
                .execution
                .as_ref()
                .is_some_and(|execution| execution.connection_id == id)
        })
        .map(|record| record.name.clone())
        .collect();
    store.registry.remove(&id, expected_revision, &references)?;
    let keys: Vec<_> = store
        .credentials
        .keys()
        .filter(|key| key.ends_with(&format!(":{id}")))
        .cloned()
        .collect();
    for key in keys {
        if let Some(reference) = store.credentials.remove(&key) {
            store.pending_secret_cleanup.push(reference);
        }
    }
    connections::persist(&app, &store)
}

/// Return a path only. Key material never passes through the media/upload pipeline.
#[tauri::command]
pub async fn pick_connection_file(
    directory: bool,
    app: AppHandle,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let picker = app.dialog().file();
    if directory {
        picker.pick_folder(move |path| {
            let _ = sender.send(path);
        });
    } else {
        picker.pick_file(move |path| {
            let _ = sender.send(path);
        });
    }
    let chosen = receiver
        .await
        .map_err(|_| "File picker closed unexpectedly".to_string())?;
    chosen
        .map(|path| {
            path.into_path()
                .map(|path| path.to_string_lossy().into_owned())
                .map_err(|e| e.to_string())
        })
        .transpose()
}

/// Check a draft without saving it. Secret inputs live only for this operation.
#[tauri::command]
pub async fn test_execution_connection(
    connection: Connection,
    input: connections::probe::ProbeInput,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<connections::probe::ProbeResult, String> {
    static LIMIT: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(4);
    let _permit = LIMIT
        .try_acquire()
        .map_err(|_| "Other connection checks are running. Try again shortly.".to_string())?;
    connection.validate()?;
    let owner = super::agents::workspace_owner_hex(&state)?;
    let mut answers = {
        let _guard = state
            .managed_agents_store_lock
            .lock()
            .map_err(|e| e.to_string())?;
        let store = connections::load(&app)?;
        if store
            .registry
            .connections
            .iter()
            .any(|item| item.id == connection.id && item.target == connection.target)
        {
            connections::credentials::answers(&store, &owner, &connection.id)?
        } else {
            buzz_connections::prompt::Answers::default()
        }
    };
    if !input.password.is_empty() {
        answers.password = zeroize::Zeroizing::new(input.password.clone());
    }
    if !input.passphrase.is_empty() {
        answers.passphrase = zeroize::Zeroizing::new(input.passphrase.clone());
    }
    answers
        .trusted_prompts
        .extend(input.approved_host_prompts.iter().cloned());
    let result = connections::probe::inspect(app, &connection, answers).await?;
    if super::agents::workspace_owner_hex(&state)? != owner {
        return Err("Identity changed during the connection check".into());
    }
    connections::probe::remember(owner, &connection, result)
}
