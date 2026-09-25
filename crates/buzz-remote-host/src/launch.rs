//! Validate identity and construct a launch snapshot without running anything.
use crate::config::{self, Config};
use buzz_connections::{
    model::WorkingDirectory,
    wire::{DeploymentScope, LaunchReceipt, LaunchState, HOST_PROTOCOL},
};
use nostr::{FromBech32, Keys};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};

/// Secret-bearing request. Deliberately not Debug.
#[derive(Deserialize)]
pub struct Deploy {
    pub protocol: u32,
    pub request_id: String,
    pub server_id: String,
    pub harness_id: String,
    pub directory: WorkingDirectory,
    pub agent: Agent,
}

/// Desktop-resolved policy and agent identity, compatible with its launch payload.
#[derive(Deserialize)]
pub struct Agent {
    pub relay_url: String,
    pub private_key_nsec: String,
    pub pubkey: Option<String>,
    pub auth_tag: Option<String>,
    pub provider: Option<String>,
    pub respond_to: Option<String>,
    #[serde(default)]
    pub respond_to_allowlist: Vec<String>,
    pub launch: Launch,
}

/// Model settings are already resolved by Desktop; no remapping in the host.
#[derive(Deserialize)]
pub struct Launch {
    pub owner_pubkey: String,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub policy_env: BTreeMap<String, String>,
}

/// Private snapshot loaded by a system supervisor. Never return or log it.
#[derive(Serialize, Deserialize)]
pub struct Snapshot {
    pub receipt: LaunchReceipt,
    pub acp_binary: PathBuf,
    pub environment: BTreeMap<String, String>,
}

/// The same canonical relay identity is used for allowlisting and deployment scope.
pub use buzz_connections::wire::canonical_relay;

/// Fail closed before creating a deployment or writing any key.
pub fn snapshot(config: &Config, request: &Deploy) -> Result<Snapshot, String> {
    if request.protocol != HOST_PROTOCOL || request.server_id != config.server_id {
        return Err("Server identity or protocol changed; check the connection again".into());
    }
    uuid::Uuid::parse_str(&request.request_id)
        .map_err(|_| "Invalid launch request ID".to_string())?;
    let keys = nostr::SecretKey::from_bech32(&request.agent.private_key_nsec)
        .map(Keys::new)
        .map_err(|_| "Invalid agent identity".to_string())?;
    let pubkey = keys.public_key().to_hex();
    if request
        .agent
        .pubkey
        .as_ref()
        .is_some_and(|value| value != &pubkey)
    {
        return Err("Agent identity does not match its key".into());
    }
    let owner = &request.agent.launch.owner_pubkey;
    if owner.len() != 64 || !owner.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("Missing or invalid agent owner".into());
    }
    if request
        .agent
        .provider
        .as_deref()
        .is_some_and(|p| p.trim() == "relay-mesh")
    {
        return Err("Shared-compute agents cannot be started through SSH".into());
    }
    let relay = canonical_relay(&request.agent.relay_url)?;
    let allowed = config
        .relay_urls
        .iter()
        .map(|url| canonical_relay(url))
        .collect::<Result<Vec<_>, _>>()?;
    if !allowed.contains(&relay) {
        return Err("This community is not enabled on the server".into());
    }
    let harness = config
        .harnesses
        .iter()
        .find(|entry| entry.catalog.id == request.harness_id)
        .ok_or("Harness is no longer installed")?;
    config::executable(&config.acp_binary)?;
    config::executable(&config.cli_binary)?;
    config::executable(&harness.executable)?;
    let directory = config.directory(&request.directory)?;
    let mut environment = request.agent.launch.policy_env.clone();
    environment.extend(request.agent.launch.env.clone());
    environment.extend(harness.environment.clone());
    if environment.len() > 256 {
        return Err("Too many environment variables".into());
    }
    for (key, value) in &environment {
        if key.is_empty()
            || !key
                .bytes()
                .enumerate()
                .all(|(i, c)| c == b'_' || c.is_ascii_alphabetic() || (i > 0 && c.is_ascii_digit()))
            || value.contains('\0')
            || value.len() > 65536
        {
            return Err("Invalid launch environment".into());
        }
    }
    environment.retain(|key, _| {
        !matches!(
            key.as_str(),
            "BUZZ_PRIVATE_KEY"
                | "NOSTR_PRIVATE_KEY"
                | "BUZZ_AUTH_TAG"
                | "BUZZ_RELAY_URL"
                | "BUZZ_ACP_PRIVATE_KEY"
                | "BUZZ_API_TOKEN"
                | "BUZZ_ACP_NO_PRESENCE"
                | "BUZZ_ACP_AGENT_OWNER"
                | "BUZZ_ACP_AGENT_COMMAND"
                | "BUZZ_ACP_AGENT_ARGS"
                | "BUZZ_ACP_MCP_COMMAND"
                | "BUZZ_ACP_RESPOND_TO"
                | "BUZZ_ACP_RESPOND_TO_ALLOWLIST"
                | "BUZZ_ACP_EXIT_AFTER_INACTIVITY"
                | "BUZZ_CLI_BINARY"
        ) && !key.starts_with("BUZZ_ACP_CHANNEL_WORKSPACE")
    });
    let mode = request.agent.respond_to.as_deref().unwrap_or("owner-only");
    if !matches!(mode, "owner-only" | "anyone" | "allowlist" | "nobody") {
        return Err("Invalid instruction access policy".into());
    }
    if mode == "allowlist"
        && (request.agent.respond_to_allowlist.is_empty()
            || request
                .agent
                .respond_to_allowlist
                .iter()
                .any(|key| key.len() != 64 || !key.bytes().all(|c| c.is_ascii_hexdigit())))
    {
        return Err("Invalid instruction allowlist".into());
    }
    environment.insert(
        "BUZZ_PRIVATE_KEY".into(),
        request.agent.private_key_nsec.clone(),
    );
    environment.insert("BUZZ_RELAY_URL".into(), relay.clone());
    environment.insert("BUZZ_ACP_AGENT_OWNER".into(), owner.to_lowercase());
    environment.insert(
        "BUZZ_ACP_AGENT_COMMAND".into(),
        harness.executable.to_string_lossy().into_owned(),
    );
    if harness.args.iter().any(|arg| arg.contains([',', '\0'])) {
        return Err("Harness arguments cannot contain commas or NUL".into());
    }
    environment.insert("BUZZ_ACP_AGENT_ARGS".into(), harness.args.join(","));
    if let Some(binary) = config.mcp_binary(harness)? {
        environment.insert(
            "BUZZ_ACP_MCP_COMMAND".into(),
            binary.to_string_lossy().into_owned(),
        );
    }
    environment.insert("BUZZ_ACP_RESPOND_TO".into(), mode.into());
    environment.insert(
        "BUZZ_ACP_RESPOND_TO_ALLOWLIST".into(),
        request.agent.respond_to_allowlist.join(","),
    );
    environment.insert("BUZZ_ACP_EXIT_AFTER_INACTIVITY".into(), "14400".into());
    let cli_directory = config.cli_binary.parent().ok_or("Invalid CLI path")?;
    let path = environment
        .get("PATH")
        .cloned()
        .unwrap_or_else(|| "/usr/local/bin:/usr/bin:/bin".into());
    environment.insert("PATH".into(), format!("{}:{path}", cli_directory.display()));
    environment.insert(
        "BUZZ_CLI_BINARY".into(),
        config.cli_binary.to_string_lossy().into_owned(),
    );
    if let Some(tag) = &request.agent.auth_tag {
        environment.insert("BUZZ_AUTH_TAG".into(), tag.clone());
    }
    Ok(Snapshot {
        receipt: LaunchReceipt {
            request_id: request.request_id.clone(),
            scope: DeploymentScope {
                server_id: config.server_id.clone(),
                owner_pubkey: owner.to_lowercase(),
                relay_url: relay,
                agent_pubkey: pubkey,
            },
            generation: uuid::Uuid::new_v4().to_string(),
            directory,
            harness_id: request.harness_id.clone(),
            harness_label: harness.catalog.label.clone(),
            model: harness
                .catalog
                .model_env_var
                .as_ref()
                .and_then(|key| environment.get(key))
                .or_else(|| environment.get("BUZZ_ACP_MODEL"))
                .cloned(),
            state: LaunchState::Starting,
        },
        acp_binary: config.acp_binary.clone(),
        environment,
    })
}
