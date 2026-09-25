//! Display desired and applied execution without probing SSH or exposing launch secrets.
use crate::{app_state::AppState, connections, managed_agents};
use buzz_connections::{model::Target, wire::canonical_relay};
use tauri::{AppHandle, State};

/// Read snapshots only. Relay presence remains the source of conversational availability.
#[tauri::command]
pub fn get_agent_execution_status(
    pubkey: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let owner = super::agents::workspace_owner_hex(&state)?;
    let relay = canonical_relay(&crate::relay::relay_ws_url_with_override(&state))?;
    let _guard = state
        .managed_agents_store_lock
        .lock()
        .map_err(|e| e.to_string())?;
    let records = managed_agents::load_managed_agents(&app)?;
    let record = records
        .iter()
        .find(|record| record.pubkey == pubkey)
        .ok_or("Agent no longer exists")?;
    let Some(desired) = &record.execution else {
        return Ok(serde_json::Value::Null);
    };
    let store = connections::load(&app)?;
    let connection = store
        .registry
        .connections
        .iter()
        .find(|item| item.id == desired.connection_id);
    let mut result = serde_json::json!({"connection_name":connection.map(|item| &item.name),"desired":desired,"applied":null,"pending":false,"launch_unconfirmed":false,"remote":record.backend != managed_agents::BackendKind::Local});
    if connection.is_some_and(|item| item.target == Target::Local) {
        let runtimes = state
            .managed_agent_processes
            .lock()
            .map_err(|e| e.to_string())?;
        if let Some(runtime) =
            managed_agents::workspace_pair_key(&app, record).and_then(|key| runtimes.get(&key))
        {
            if let Some(execution) = &runtime.spawn_config.execution {
                result["applied"] = serde_json::json!({"harness_id":execution.harness_id,"harness_label":managed_agents::known_acp_runtime(&runtime.spawn_config.command).map(|meta| meta.label),"model":runtime.spawn_config.model,"directory":runtime.execution_directory});
                result["pending"] = serde_json::json!(execution != desired);
            }
        }
    } else {
        let attempt = store.launches.values().find(|attempt| {
            attempt.scope.owner_pubkey == owner
                && attempt.scope.relay_url == relay
                && attempt.scope.agent_pubkey == pubkey
                && attempt.execution.connection_id == desired.connection_id
        });
        if let Some(attempt) = attempt {
            result["launch_unconfirmed"] = serde_json::json!(attempt.receipt.is_none());
            if let Some(receipt) = &attempt.receipt {
                result["applied"] = serde_json::json!({"harness_id":receipt.harness_id,"harness_label":receipt.harness_label,"model":receipt.model,"directory":receipt.directory});
                result["pending"] = serde_json::json!(pending_execution(
                    desired,
                    &attempt.execution,
                    &receipt.harness_id,
                    receipt.model.as_deref()
                ));
            }
        }
    }
    Ok(result)
}

fn pending_execution(
    desired: &buzz_connections::model::Execution,
    captured: &buzz_connections::model::Execution,
    harness: &str,
    model: Option<&str>,
) -> bool {
    desired != captured
        || desired.harness_id.as_deref() != Some(harness)
        || desired
            .model
            .as_deref()
            .is_some_and(|selected| Some(selected) != model)
}

#[cfg(test)]
mod tests {
    use super::*;
    use buzz_connections::model::{Execution, WorkingDirectory};
    #[test]
    fn resolved_harness_default_is_not_a_pending_user_edit() {
        let captured = Execution {
            connection_id: "connection".into(),
            harness_id: Some("codex".into()),
            model: None,
            directory: WorkingDirectory::Automatic,
        };
        assert!(!pending_execution(
            &captured,
            &captured,
            "codex",
            Some("server-default")
        ));
        let mut changed = captured.clone();
        changed.model = Some("chosen-model".into());
        assert!(pending_execution(
            &changed,
            &captured,
            "codex",
            Some("server-default")
        ));
        assert!(!pending_execution(
            &changed,
            &changed,
            "codex",
            Some("chosen-model")
        ));
        assert!(pending_execution(
            &changed,
            &changed,
            "codex",
            Some("different")
        ));
    }
}
