//! Machine-specific configuration is separate from portable agent identity.
use crate::{
    app_state::AppState,
    managed_agents::{self, BackendKind, ManagedAgentSummary},
};
use buzz_connections::model::{Connection, Execution, Target};
use tauri::{AppHandle, State};
pub(crate) fn backend_for(connection: &Connection) -> BackendKind {
    match connection.target {
        Target::Local => BackendKind::Local,
        Target::Ssh { .. } => BackendKind::Provider {
            id: "ssh-connections".into(),
            config: serde_json::json!({"connection_id":connection.id}),
        },
    }
}
/// Saving execution never restarts a running agent. A different machine requires a stopped instance.
#[tauri::command]
pub fn set_agent_execution(
    pubkey: String,
    execution: Execution,
    expected_updated_at: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ManagedAgentSummary, String> {
    let _guard = state
        .managed_agents_store_lock
        .lock()
        .map_err(|e| e.to_string())?;
    let mut records = managed_agents::load_managed_agents(&app)?;
    let record = managed_agents::find_managed_agent_mut(&mut records, &pubkey)?;
    apply_execution(&app, record, execution, &expected_updated_at)?;
    record.updated_at = crate::util::now_iso();
    let record = record.clone();
    managed_agents::save_managed_agents(&app, &records)?;
    let runtimes = state
        .managed_agent_processes
        .lock()
        .map_err(|e| e.to_string())?;
    super::agents::summarize_from_disk(&app, &record, &runtimes)
}

/// Caller holds the managed-agent store lock; validation precedes persistence.
pub(crate) fn apply_execution(
    app: &AppHandle,
    record: &mut managed_agents::ManagedAgentRecord,
    execution: Execution,
    expected_updated_at: &str,
) -> Result<(), String> {
    let connection = crate::connections::get(app, &execution.connection_id)?;
    execution.validate(&connection)?;
    if record.updated_at != expected_updated_at {
        return Err("Agent changed. Reload and try again.".into());
    }
    let changed_machine = record.execution.as_ref().map(|value| &value.connection_id)
        != Some(&execution.connection_id);
    let uncertain_launch = crate::connections::load(app)?.has_unsettled_launch(&record.pubkey);
    if changed_machine
        && (record.runtime_pid.is_some() || record.backend_agent_id.is_some() || uncertain_launch)
    {
        return Err(
            "Stop this agent and confirm its stopped state before changing connections.".into(),
        );
    }
    record.backend = backend_for(&connection);
    record.execution = Some(execution);
    record.start_on_app_launch = false;
    record.auto_restart_on_config_change = false;
    Ok(())
}

/// Explicit recovery query after relay Stop; never starts or terminates a remote process.
#[tauri::command]
pub async fn confirm_execution_stopped(
    pubkey: String,
    input: Option<crate::connections::probe::ProbeInput>,
    expected_relay_url: String,
    expected_signer_pubkey: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ManagedAgentSummary, String> {
    crate::connections::launch::start(
        &app,
        &state,
        &pubkey,
        input.unwrap_or_default(),
        &expected_relay_url,
        &expected_signer_pubkey,
        None,
        crate::connections::launch::Action::ConfirmStopped,
    )
    .await
}

/// Keep the last recovery record until an uncertain launch has been resolved.
pub(crate) fn ensure_deletion_safe(
    record: &managed_agents::ManagedAgentRecord,
    uncertain_launch: bool,
    force_remote_delete: bool,
) -> Result<(), String> {
    if record.execution.is_some() && uncertain_launch {
        return Err("A launch may still be active. Stop and confirm this agent in each community before deleting it.".into());
    }
    if record.backend != BackendKind::Local
        && record.backend_agent_id.is_some()
        && !force_remote_delete
    {
        return Err(
            "cannot delete a deployed remote agent without force_remote_delete: true".into(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn uncertain_launch_cannot_be_orphaned_even_by_force_delete() {
        let mut record: managed_agents::ManagedAgentRecord =
            serde_json::from_value(serde_json::json!({
                "pubkey":"agent", "name":"Agent", "relay_url":"", "acp_command":"",
                "agent_command":"", "agent_args":[], "mcp_command":"", "turn_timeout_seconds":0,
                "system_prompt":null,"created_at":"","updated_at":"","last_started_at":null,
                "last_stopped_at":null,"last_exit_code":null,"last_error":null
            }))
            .unwrap();
        record.backend = BackendKind::Provider {
            id: "provider-v1".into(),
            config: serde_json::json!({}),
        };
        record.backend_agent_id = Some("deployed".into());
        assert!(ensure_deletion_safe(&record, false, false).is_err());
        assert!(ensure_deletion_safe(&record, false, true).is_ok());
        record.execution = Some(Execution {
            connection_id: uuid::Uuid::new_v4().to_string(),
            harness_id: None,
            model: None,
            directory: buzz_connections::model::WorkingDirectory::Automatic,
        });
        record.backend_agent_id = None;
        assert!(ensure_deletion_safe(&record, true, false).is_err());
        assert!(ensure_deletion_safe(&record, true, true).is_err());
        assert!(ensure_deletion_safe(&record, false, false).is_ok());
    }
}
