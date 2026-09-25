//! Native SSH handoff: negotiate, freeze one request, then rely on relay presence.
use super::{probe, Store};
use crate::{
    app_state::AppState,
    managed_agents::{self, ManagedAgentRecord, ManagedAgentSummary},
};
use buzz_connections::{
    catalog::{AcpAvailabilityStatus, AuthStatus},
    model::{Connection, Execution, Target},
    wire::{DeploymentScope, HostInfo, LaunchReceipt, LaunchState, HOST_PROTOCOL},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::AppHandle;

/// Durable, secret-free recovery record. A timeout never erases the request identity.
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Attempt {
    pub scope: DeploymentScope,
    pub request_id: String,
    pub execution: Execution,
    pub receipt: Option<LaunchReceipt>,
}
fn failure(error: buzz_connections::remote::Failure) -> String {
    match error.outcome {
        buzz_connections::model::CheckOutcome::AuthenticationRequired => {
            "SSH_AUTH_REQUIRED: Enter credentials to start this agent.".into()
        }
        buzz_connections::model::CheckOutcome::HostTrustRequired { .. } => {
            "Check this connection and confirm its server fingerprint before starting.".into()
        }
        buzz_connections::model::CheckOutcome::HostKeyChanged => {
            "Server identity changed. Verify the connection before starting.".into()
        }
        buzz_connections::model::CheckOutcome::SetupRequired => {
            "Set up Buzz on this server before starting.".into()
        }
        buzz_connections::model::CheckOutcome::Failed { message } => message,
        _ => "SSH connection failed. Check the server and retry.".into(),
    }
}
fn current(
    app: &AppHandle,
    state: &AppState,
    pubkey: &str,
) -> Result<(ManagedAgentRecord, Connection, Store), String> {
    let _guard = state
        .managed_agents_store_lock
        .lock()
        .map_err(|e| e.to_string())?;
    let record = managed_agents::load_managed_agents(app)?
        .into_iter()
        .find(|record| record.pubkey == pubkey)
        .ok_or("Agent no longer exists")?;
    let execution = record
        .execution
        .as_ref()
        .ok_or("Choose a connection before starting")?;
    let store = super::load(app)?;
    let connection = store
        .registry
        .connections
        .iter()
        .find(|item| item.id == execution.connection_id)
        .cloned()
        .ok_or("Connection no longer exists")?;
    execution.validate(&connection)?;
    Ok((record, connection, store))
}
pub(crate) enum Action {
    Start,
    ConfirmStopped,
}
/// Called only for an explicit Start. No reconnect loop or background SSH polling.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn start(
    app: &AppHandle,
    state: &AppState,
    pubkey: &str,
    input: probe::ProbeInput,
    expected_relay: &str,
    expected_owner: &str,
    replay_floor: Option<u64>,
    action: Action,
) -> Result<ManagedAgentSummary, String> {
    let lock = {
        let mut locks = state
            .provider_deploy_locks
            .lock()
            .map_err(|e| e.to_string())?;
        Arc::clone(
            locks
                .entry(pubkey.into())
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(()))),
        )
    };
    let _launch_guard = lock.lock().await;
    let (record, connection, store) = current(app, state, pubkey)?;
    let execution = record.execution.as_ref().ok_or("Execution is missing")?;
    let Target::Ssh { endpoint } = &connection.target else {
        return Err("This connection is not SSH".into());
    };
    let owner = crate::commands::agents::workspace_owner_hex(state)?;
    crate::relay::assert_expected_signer(Some(expected_owner), &owner)?;
    crate::relay::assert_expected_relay_scope(
        Some(expected_relay),
        &crate::relay::relay_api_base_url_with_override(state),
    )?;
    let mut answers = probe::answers_for_input(&store, &owner, &connection.id, &input)
        .map_err(|_| "SSH_AUTH_REQUIRED: Saved SSH credentials are unavailable. Enter credentials for this operation.".to_string())?;
    if !input.password.is_empty() {
        answers.password = zeroize::Zeroizing::new(input.password.clone());
    }
    if !input.passphrase.is_empty() {
        answers.passphrase = zeroize::Zeroizing::new(input.passphrase.clone());
    }
    answers
        .trusted_prompts
        .extend(input.approved_host_prompts.iter().cloned());
    let helper = probe::helper()?;
    let info = buzz_connections::remote::call(
        endpoint,
        &helper,
        answers.clone(),
        &serde_json::json!({"op":"info"}),
    )
    .await
    .map_err(failure)?;
    if info.get("protocol").and_then(|value| value.as_u64()) != Some(HOST_PROTOCOL.into()) {
        return Err("Update the server component before starting".into());
    }
    let info: HostInfo = serde_json::from_value(info).map_err(|_| "Invalid server catalog")?;
    uuid::Uuid::parse_str(&info.server_id).map_err(|_| "Invalid server identity")?;
    if connection
        .check
        .as_ref()
        .and_then(|check| check.server_id.as_ref())
        .is_some_and(|id| id != &info.server_id)
    {
        return Err("Server identity changed. Check and save the connection again.".into());
    }
    let relay = crate::relay::effective_agent_relay_url(
        &record.relay_url,
        &crate::relay::relay_ws_url_with_override(state),
    );
    let relay = buzz_connections::wire::canonical_relay(&relay)?;
    let scope = DeploymentScope {
        server_id: info.server_id.clone(),
        owner_pubkey: owner.clone(),
        relay_url: relay.clone(),
        agent_pubkey: pubkey.into(),
    };
    let scope_key = scope.key();
    if let Some(previous) = store.launches.get(&scope_key) {
        let status = buzz_connections::remote::call(
            endpoint,
            &helper,
            answers.clone(),
            &serde_json::json!({"op":"status","protocol":HOST_PROTOCOL,"scope":scope}),
        )
        .await
        .map_err(failure)?;
        if !status.is_null() {
            let receipt: LaunchReceipt =
                serde_json::from_value(status).map_err(|_| "Invalid launch status")?;
            if receipt.scope != scope {
                return Err("Launch status belongs to another scope".into());
            }
            if matches!(action, Action::ConfirmStopped)
                && !matches!(receipt.state, LaunchState::Stopped | LaunchState::Failed)
            {
                return Err("This instance is still running. Stop it through Buzz before changing connections.".into());
            }
            if matches!(action, Action::ConfirmStopped)
                || matches!(
                    receipt.state,
                    LaunchState::Running | LaunchState::Starting | LaunchState::Unconfirmed
                )
                || previous.receipt.is_none()
            {
                return record_receipt(
                    app,
                    state,
                    pubkey,
                    &scope_key,
                    previous.execution.clone(),
                    receipt,
                );
            }
        }
    }
    if matches!(action, Action::ConfirmStopped) {
        return Err("No confirmed launch record is available. Recover this agent's launch status before changing connections.".into());
    }
    let harness = info
        .harnesses
        .iter()
        .find(|harness| Some(&harness.id) == execution.harness_id.as_ref())
        .ok_or("Choose an available server harness in Execution")?;
    if harness.availability != AcpAvailabilityStatus::Available
        || !matches!(
            harness.auth_status,
            AuthStatus::LoggedIn | AuthStatus::NotApplicable
        )
    {
        return Err("The server harness needs setup or sign-in before starting".into());
    }
    let (request_id, payload) = {
        let _guard = state
            .managed_agents_store_lock
            .lock()
            .map_err(|e| e.to_string())?;
        crate::relay::assert_expected_signer(
            Some(expected_owner),
            &crate::commands::agents::workspace_owner_hex(state)?,
        )?;
        crate::relay::assert_expected_relay_scope(
            Some(expected_relay),
            &crate::relay::relay_api_base_url_with_override(state),
        )?;
        let records = managed_agents::load_managed_agents(app)?;
        let fresh = records
            .iter()
            .find(|item| item.pubkey == pubkey)
            .ok_or("Agent no longer exists")?;
        if fresh.updated_at != record.updated_at || fresh.execution != record.execution {
            return Err("Agent changed while checking. Start again.".into());
        }
        let mut store = super::load(app)?;
        if store
            .registry
            .connections
            .iter()
            .find(|item| item.id == connection.id)
            .is_none_or(|item| item.revision != connection.revision)
        {
            return Err("Connection changed while checking. Start again.".into());
        }
        let id = store
            .launches
            .get(&scope_key)
            .filter(|attempt| attempt.receipt.is_none())
            .map(|attempt| attempt.request_id.clone())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        if !store.launches.contains_key(&scope_key) && store.launches.len() >= 1024 {
            return Err("Launch recovery store is full".into());
        }
        let mut payload =
            crate::commands::agents::build_connection_deploy_payload(app, state, fresh, harness)?;
        if let Some(floor) = replay_floor {
            payload["launch"]["policy_env"]["BUZZ_ACP_REPLAY_FLOOR"] =
                serde_json::json!(floor.to_string());
            if let Some(env) = payload["launch"]["env"].as_object_mut() {
                env.remove("BUZZ_ACP_REPLAY_FLOOR");
            }
        }
        store.launches.insert(
            scope_key.clone(),
            Attempt {
                scope: scope.clone(),
                request_id: id.clone(),
                execution: execution.clone(),
                receipt: None,
            },
        );
        super::persist(app, &store)?;
        (id, payload)
    };
    let request = serde_json::json!({"op":"deploy","protocol":HOST_PROTOCOL,"request_id":request_id,"server_id":info.server_id,"harness_id":harness.id,"directory":execution.directory,"agent":payload});
    let receipt = buzz_connections::remote::call(endpoint,&helper,answers,&request).await.map_err(|_| "LAUNCH_UNCONFIRMED: The launch result is unknown. Retry checks the existing request before starting another instance.".to_string())?;
    let receipt: LaunchReceipt = serde_json::from_value(receipt)
        .map_err(|_| "LAUNCH_UNCONFIRMED: Invalid launch receipt. Retry to recover status.")?;
    if receipt.scope != scope {
        return Err("LAUNCH_UNCONFIRMED: Receipt scope mismatch".into());
    }
    record_receipt(app, state, pubkey, &scope_key, execution.clone(), receipt)
}
fn record_receipt(
    app: &AppHandle,
    state: &AppState,
    pubkey: &str,
    key: &str,
    execution: Execution,
    receipt: LaunchReceipt,
) -> Result<ManagedAgentSummary, String> {
    let _guard = state
        .managed_agents_store_lock
        .lock()
        .map_err(|e| e.to_string())?;
    let mut store = super::load(app)?;
    store.launches.insert(
        key.into(),
        Attempt {
            scope: receipt.scope.clone(),
            request_id: receipt.request_id.clone(),
            execution,
            receipt: Some(receipt.clone()),
        },
    );
    super::persist(app, &store)?;
    // Recovery bookkeeping is durable even if the UI identity changes. Do not
    // apply that receipt to another active identity/community or reassigned instance.
    crate::relay::assert_expected_signer(
        Some(&receipt.scope.owner_pubkey),
        &crate::commands::agents::workspace_owner_hex(state)?,
    )?;
    crate::relay::assert_expected_relay_scope(
        Some(&receipt.scope.relay_url),
        &crate::relay::relay_api_base_url_with_override(state),
    )?;
    let mut records = managed_agents::load_managed_agents(app)?;
    let record = managed_agents::find_managed_agent_mut(&mut records, pubkey)?;
    if record.execution.as_ref().map(|value| &value.connection_id)
        != store
            .launches
            .get(key)
            .map(|attempt| &attempt.execution.connection_id)
    {
        return Err("Agent assignment changed. Launch receipt retained for recovery.".into());
    }
    record.backend_agent_id = if matches!(receipt.state, LaunchState::Stopped | LaunchState::Failed)
    {
        None
    } else {
        Some(receipt.generation)
    };
    record.last_error = None;
    record.last_error_code = None;
    record.last_started_at = Some(crate::util::now_iso());
    let record = record.clone();
    managed_agents::save_managed_agents(app, &records)?;
    let runtimes = state
        .managed_agent_processes
        .lock()
        .map_err(|e| e.to_string())?;
    crate::commands::agents::summarize_from_disk(app, &record, &runtimes)
}
