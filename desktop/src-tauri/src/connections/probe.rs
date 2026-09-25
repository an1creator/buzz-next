//! Ephemeral check tickets bind a result to its exact owner and transport settings.
use buzz_connections::{
    catalog::{AcpAvailabilityStatus, AcpRuntimeCatalogEntry, AuthStatus},
    model::{CheckOutcome, Connection, ConnectionCheck, Target},
    prompt::Answers,
    wire::{HostInfo, HOST_PROTOCOL},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::AppHandle;

/// Transient inputs; passwords are never part of connection metadata.
#[derive(Default, Deserialize)]
pub(crate) struct ProbeInput {
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub passphrase: String,
    #[serde(default)]
    pub approved_host_prompts: Vec<String>,
}
impl Drop for ProbeInput {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.password.zeroize();
        self.passphrase.zeroize();
    }
}

/// The UI receives a ticket, never authority to manufacture its own readiness.
#[derive(Clone, Serialize)]
pub(crate) struct ProbeResult {
    pub check: ConnectionCheck,
    pub harnesses: Vec<AcpRuntimeCatalogEntry>,
    pub trust_prompt: Option<String>,
    pub ticket: String,
}
struct Ticket {
    owner: String,
    target: Target,
    result: ProbeResult,
    created: Instant,
    credential_digest: String,
}
fn tickets() -> &'static Mutex<HashMap<String, Ticket>> {
    static TICKETS: OnceLock<Mutex<HashMap<String, Ticket>>> = OnceLock::new();
    TICKETS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(crate) fn remember(
    owner: String,
    connection: &Connection,
    mut result: ProbeResult,
    credentials: (&str, &str),
) -> Result<ProbeResult, String> {
    let mut tickets = tickets().lock().map_err(|e| e.to_string())?;
    tickets.retain(|_, ticket| ticket.created.elapsed() < Duration::from_secs(300));
    if tickets.len() >= 128 {
        return Err("Too many connection checks. Wait a moment and retry.".into());
    }
    result.ticket = uuid::Uuid::new_v4().to_string();
    tickets.insert(
        result.ticket.clone(),
        Ticket {
            owner,
            target: connection.target.clone(),
            result: result.clone(),
            created: Instant::now(),
            credential_digest: credential_digest(&result.ticket, credentials),
        },
    );
    Ok(result)
}

pub(crate) fn checked(
    owner: &str,
    connection: &Connection,
    ticket: &str,
    credentials: Option<&super::SavedCredentials>,
) -> Result<ConnectionCheck, String> {
    let tickets = tickets().lock().map_err(|e| e.to_string())?;
    let ticket = tickets
        .get(ticket)
        .ok_or("Connection check expired. Check again.")?;
    if ticket.owner != owner
        || ticket.target != connection.target
        || ticket.created.elapsed() >= Duration::from_secs(300)
    {
        return Err("Connection changed or check expired. Check again.".into());
    }
    if credentials.is_some_and(|credentials| {
        credential_digest(
            &ticket.result.ticket,
            (&credentials.password, &credentials.passphrase),
        ) != ticket.credential_digest
    }) {
        return Err("Credentials changed after the check. Check again.".into());
    }
    let mut check = ticket.result.check.clone();
    check.revision = connection.revision;
    Ok(check)
}

pub(crate) fn helper() -> Result<std::path::PathBuf, String> {
    let executable = std::env::current_exe().map_err(|_| "Cannot locate SSH helper".to_string())?;
    let parent = executable.parent().ok_or("Cannot locate SSH helper")?;
    Ok(parent.join(if cfg!(windows) {
        "buzz-ssh-askpass.exe"
    } else {
        "buzz-ssh-askpass"
    }))
}

pub(crate) async fn inspect(
    app: AppHandle,
    connection: &Connection,
    answers: Answers,
) -> Result<ProbeResult, String> {
    let checked_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs();
    let mut result = ProbeResult {
        check: ConnectionCheck {
            revision: connection.revision,
            checked_at,
            outcome: CheckOutcome::Unreachable,
            server_id: None,
            default_directory: None,
        },
        harnesses: Vec::new(),
        trust_prompt: None,
        ticket: String::new(),
    };
    match &connection.target {
        Target::Local => {
            result.harnesses = crate::commands::discover_acp_providers(app, Some(true)).await?;
            result.check.default_directory = crate::managed_agents::default_agent_workdir()
                .map(|p| p.to_string_lossy().into_owned());
        }
        Target::Ssh { endpoint } => {
            match buzz_connections::remote::call(
                endpoint,
                &helper()?,
                answers,
                &serde_json::json!({"op":"info"}),
            )
            .await
            {
                Ok(value) => {
                    if value.get("protocol").and_then(|v| v.as_u64()) != Some(HOST_PROTOCOL.into())
                    {
                        result.check.outcome = CheckOutcome::UpdateRequired {
                            required_protocol: HOST_PROTOCOL,
                        };
                        return Ok(result);
                    }
                    let info: HostInfo = serde_json::from_value(value)
                        .map_err(|_| "Server catalog is invalid".to_string())?;
                    result.check.server_id = Some(info.server_id);
                    result.check.default_directory = Some(info.default_directory);
                    result.harnesses = info.harnesses;
                }
                Err(failure) => {
                    result.check.outcome = failure.outcome;
                    result.trust_prompt = failure.trust_prompt;
                    return Ok(result);
                }
            }
        }
    }
    result.check.outcome = if result.harnesses.iter().any(|entry| {
        entry.availability == AcpAvailabilityStatus::Available
            && matches!(
                entry.auth_status,
                AuthStatus::LoggedIn | AuthStatus::NotApplicable
            )
    }) {
        CheckOutcome::Ready
    } else {
        CheckOutcome::HarnessSetupRequired
    };
    Ok(result)
}

fn credential_digest(ticket: &str, credentials: (&str, &str)) -> String {
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    for value in [ticket, credentials.0, credentials.1] {
        hash.update((value.len() as u64).to_be_bytes());
        hash.update(value.as_bytes());
    }
    hex::encode(hash.finalize())
}
