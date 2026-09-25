//! Model discovery is scoped to a saved connection, never to the launcher's machine.
use crate::{app_state::AppState, connections, managed_agents::AgentModelsResponse};
use buzz_connections::{model::Target, wire::HOST_PROTOCOL};
use tauri::{AppHandle, State};

/// Query a selected harness without starting a conversational agent.
#[tauri::command]
pub async fn get_connection_models(
    connection_id: String,
    expected_revision: u64,
    harness_id: String,
    operation_id: String,
    input: connections::probe::ProbeInput,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<AgentModelsResponse, String> {
    let owner = super::agents::workspace_owner_hex(&state)?;
    let (connection, mut answers) = {
        let _guard = state
            .managed_agents_store_lock
            .lock()
            .map_err(|e| e.to_string())?;
        let connection = connections::get(&app, &connection_id)?;
        if connection.revision != expected_revision {
            return Err("Connection changed. Reload and retry.".into());
        }
        let answers = connections::probe::answers_for_input(
            &connections::load(&app)?,
            &owner,
            &connection_id,
            &input,
        )?;
        (connection, answers)
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
    let (_operation, mut cancellation) = connections::operations::begin(&owner, &operation_id)?;
    let query = async {
        match &connection.target {
            Target::Local => {
                let catalog = super::discover_acp_providers(app.clone(), Some(false)).await?;
                let harness = catalog
                    .into_iter()
                    .find(|item| item.id == harness_id)
                    .ok_or("Harness is no longer available on this device")?;
                super::agent_models::discover_agent_models(
                    super::agent_models::DiscoverAgentModelsInput {
                        acp_command: None,
                        agent_command: harness
                            .command
                            .ok_or("Install this harness before discovering models")?,
                        agent_args: harness.default_args,
                        provider: None,
                        env_vars: Default::default(),
                        definition_env: harness.definition_env,
                    },
                    state.clone(),
                )
                .await
            }
            Target::Ssh { endpoint } => {
                let raw = buzz_connections::remote::call(endpoint, &connections::probe::helper()?, answers,
                    &serde_json::json!({"op":"models", "protocol":HOST_PROTOCOL,"harness_id":harness_id})).await
                    .map_err(|failure| match failure.outcome {
                        buzz_connections::model::CheckOutcome::Failed { message } => message,
                        _ => "Check this SSH connection before discovering models".into(),
                    })?;
                Ok(super::agent_models::normalize_agent_models(&raw, None))
            }
        }
    };
    let result = tokio::select! {
        result = query => result?,
        _ = cancellation.changed() => return Err("Model discovery cancelled".into()),
    };
    if super::agents::workspace_owner_hex(&state)? != owner {
        return Err("Identity changed during discovery".into());
    }
    let _guard = state
        .managed_agents_store_lock
        .lock()
        .map_err(|e| e.to_string())?;
    if connections::get(&app, &connection_id)?.revision != expected_revision {
        return Err("Connection changed during discovery".into());
    }
    Ok(result)
}
