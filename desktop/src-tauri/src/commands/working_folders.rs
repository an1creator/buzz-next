//! Save-gated, owner/community-scoped working-folder preferences.
use crate::{
    app_state::AppState,
    managed_agents::{self, workspaces, BackendKind, ManagedAgentRecord},
};
use serde::Serialize;
use tauri::{AppHandle, State};
use uuid::Uuid;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkingFolderTarget {
    id: String,
    agent_pubkey: String,
    label: String,
}

fn bound_scope(state: &AppState, relay: &str, owner: &str) -> Result<(String, String), String> {
    if relay.trim().is_empty() || owner.trim().is_empty() {
        return Err("Select a community and identity first".into());
    }
    let relay = crate::relay::bind_expected_relay_scope(
        Some(relay),
        crate::relay::relay_ws_url_with_override(state),
    )?;
    let owner = crate::relay::bind_expected_signer(
        Some(owner),
        super::agents::workspace_owner_hex(state)?,
    )?;
    Ok((relay.as_str().into(), owner.as_str().into()))
}

fn record(app: &AppHandle, pubkey: &str) -> Result<ManagedAgentRecord, String> {
    managed_agents::load_managed_agents(app)?
        .into_iter()
        .find(|r| r.pubkey == pubkey)
        .ok_or_else(|| "This agent is no longer available".into())
}

#[tauri::command]
pub fn working_folder_targets(
    app: AppHandle,
    state: State<'_, AppState>,
    expected_relay_url: String,
    expected_signer_pubkey: String,
) -> Result<Vec<WorkingFolderTarget>, String> {
    bound_scope(&state, &expected_relay_url, &expected_signer_pubkey)?;
    let mut targets = std::collections::BTreeMap::new();
    for record in managed_agents::load_managed_agents(&app)? {
        let id = workspaces::target_id(&record.backend)?;
        let label = match &record.backend {
            BackendKind::Local => "This computer".to_string(),
            BackendKind::Provider { id, config } => {
                let host = config.get("host").and_then(|v| v.as_str()).unwrap_or(id);
                match config.get("profile").and_then(|v| v.as_str()) {
                    Some(profile) => format!("{host} · {profile}"),
                    None => host.to_string(),
                }
            }
        };
        let stock_local = record.backend == BackendKind::Local
            && std::path::Path::new(&record.acp_command)
                .file_stem()
                .is_some_and(|s| s == "buzz-acp");
        let target = WorkingFolderTarget {
            id: id.clone(),
            agent_pubkey: record.pubkey,
            label,
        };
        if stock_local {
            targets.insert(id, target);
        } else {
            targets.entry(id).or_insert(target);
        }
    }
    Ok(targets.into_values().collect())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkingFolderSettings {
    target_id: String,
    supported: bool,
    default_env_key: &'static str,
    remote: bool,
    folder: Option<workspaces::ChannelFolder>,
}

fn supports(record: &ManagedAgentRecord) -> Result<bool, String> {
    match &record.backend {
        BackendKind::Local => Ok(std::path::Path::new(&record.acp_command)
            .file_stem()
            .is_some_and(|s| s == "buzz-acp")),
        BackendKind::Provider { id, .. } => {
            let binary = managed_agents::resolve_provider_binary(id)?;
            let info = managed_agents::invoke_provider(
                &binary,
                &serde_json::json!({"op":"info","request_id":Uuid::new_v4()}),
                std::time::Duration::from_secs(10),
            )?;
            Ok(info.get("workspace_protocol").and_then(|v| v.as_u64()) == Some(1))
        }
    }
}

#[tauri::command]
pub async fn get_working_folder_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    agent_pubkey: String,
    channel_id: Option<Uuid>,
    expected_relay_url: String,
    expected_signer_pubkey: String,
) -> Result<WorkingFolderSettings, String> {
    let (relay, owner) = bound_scope(&state, &expected_relay_url, &expected_signer_pubkey)?;
    let record = record(&app, &agent_pubkey)?;
    tokio::task::spawn_blocking(move || {
        let supported = supports(&record)?;
        Ok(WorkingFolderSettings {
            target_id: workspaces::target_id(&record.backend)?,
            supported,
            default_env_key: buzz_workspaces::DEFAULT_ENV,
            remote: record.backend != BackendKind::Local,
            folder: channel_id
                .map(|channel| workspaces::channel(&app, &owner, &relay, &record.backend, channel))
                .transpose()?,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn set_channel_working_folder(
    app: AppHandle,
    state: State<'_, AppState>,
    agent_pubkey: String,
    target_id: String,
    channel_id: Uuid,
    path: Option<String>,
    revision: u64,
    expected_relay_url: String,
    expected_signer_pubkey: String,
) -> Result<workspaces::ChannelFolder, String> {
    let (relay, owner) = bound_scope(&state, &expected_relay_url, &expected_signer_pubkey)?;
    let candidate = record(&app, &agent_pubkey)?;
    let candidate_command = candidate.acp_command.clone();
    if workspaces::target_id(&candidate.backend)? != target_id {
        return Err("The agent's computer changed. Reload before saving.".into());
    }
    if !tokio::task::spawn_blocking(move || supports(&candidate))
        .await
        .map_err(|e| e.to_string())??
    {
        return Err("This runtime does not support working folders".into());
    }
    // Rebind after the provider probe; it must not retarget an old dialog.
    bound_scope(&state, &expected_relay_url, &expected_signer_pubkey)?;
    let _guard = state
        .managed_agents_store_lock
        .lock()
        .map_err(|e| e.to_string())?;
    let record = record(&app, &agent_pubkey)?;
    if workspaces::target_id(&record.backend)? != target_id
        || record.acp_command != candidate_command
    {
        return Err("The agent's computer changed. Reload before saving.".into());
    }
    workspaces::save(
        &app,
        &owner,
        &relay,
        &record.backend,
        channel_id,
        path,
        revision,
    )
}
