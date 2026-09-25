//! Directory validation runs on the selected execution machine.
use crate::{app_state::AppState, connections};
use buzz_connections::{
    model::{Connection, Execution, Target, WorkingDirectory},
    wire::{DirectoryInfo, HOST_PROTOCOL},
};
use tauri::{AppHandle, State};

/// Validate without creating folders, changing defaults, or launching an agent.
#[tauri::command]
pub async fn validate_execution_directory(
    connection_id: String,
    expected_revision: u64,
    directory: WorkingDirectory,
    input: connections::probe::ProbeInput,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<DirectoryInfo, String> {
    let owner = super::agents::workspace_owner_hex(&state)?;
    let (connection, mut answers) = {
        let _guard = state
            .managed_agents_store_lock
            .lock()
            .map_err(|e| e.to_string())?;
        let store = connections::load(&app)?;
        let connection = store
            .registry
            .connections
            .iter()
            .find(|item| item.id == connection_id)
            .cloned()
            .ok_or("Connection no longer exists")?;
        if connection.revision != expected_revision {
            return Err("Connection changed. Reload and try again.".into());
        }
        let answers =
            connections::probe::answers_for_input(&store, &owner, &connection_id, &input)?;
        (connection, answers)
    };
    Execution {
        connection_id,
        harness_id: None,
        model: None,
        directory: directory.clone(),
    }
    .validate(&connection)?;
    if !input.password.is_empty() {
        answers.password = zeroize::Zeroizing::new(input.password.clone());
    }
    if !input.passphrase.is_empty() {
        answers.passphrase = zeroize::Zeroizing::new(input.passphrase.clone());
    }
    answers
        .trusted_prompts
        .extend(input.approved_host_prompts.iter().cloned());
    let result = match &connection.target {
        Target::Local => DirectoryInfo {
            path: crate::connections::directory::local_directory(&directory)?,
        },
        Target::Ssh { endpoint } => {
            let value = buzz_connections::remote::call(endpoint, &connections::probe::helper()?, answers, &serde_json::json!({"op":"validate_directory", "protocol":HOST_PROTOCOL, "directory":directory})).await.map_err(|failure| match failure.outcome {
                buzz_connections::model::CheckOutcome::Failed { message } => message,
                _ => "Check the SSH connection before validating its directory".into(),
            })?;
            serde_json::from_value(value).map_err(|_| "Invalid directory response")?
        }
    };
    if super::agents::workspace_owner_hex(&state)? != owner {
        return Err("Identity changed during validation".into());
    }
    let _guard = state
        .managed_agents_store_lock
        .lock()
        .map_err(|e| e.to_string())?;
    let current: Connection = connections::get(&app, &connection.id)?;
    if current.revision != expected_revision {
        return Err("Connection changed during validation".into());
    }
    Ok(result)
}
