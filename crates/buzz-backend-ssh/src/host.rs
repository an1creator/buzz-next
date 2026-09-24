//! Linux host deployment. A systemd user unit owns the process after SSH exits.
use crate::{
    process,
    wire::{self, Deploy, Snapshot},
};
use fs2::FileExt;
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::process::Command;

/// Operator-installed paths and allowed communities; contains no agent keys.
#[derive(Deserialize)]
pub struct HostConfig {
    /// Private state outside installed binary versions.
    pub state: PathBuf,
    /// Communities this installation accepts.
    pub relays: Vec<String>,
    /// Server-owned launch profiles selected by provider config.
    pub profiles: BTreeMap<String, Profile>,
}
/// Paths resolved on the execution host, never copied from Windows.
#[derive(Deserialize)]
pub struct Profile {
    /// Logical Desktop command this profile implements.
    pub command_name: String,
    /// Immutable absolute buzz-acp path.
    pub harness: String,
    /// Immutable absolute Buzz CLI path.
    pub cli: String,
    /// Installed ACP command.
    pub command: String,
    /// Default working directory.
    pub workspace: String,
    /// Host-authoritative environment, including PATH and shared runtime settings.
    pub env: BTreeMap<String, String>,
}

/// Read the host's configuration without reflecting file content into an error.
pub fn config() -> Result<HostConfig, String> {
    let home = std::env::var_os("HOME").ok_or("HOME is missing")?;
    let path = PathBuf::from(home).join(".config/buzz-next/host.json");
    if fs::metadata(&path)
        .map_err(|_| "host profile is not installed")?
        .len()
        > crate::MAX_BYTES as u64
    {
        return Err("host configuration is too large".into());
    }
    let bytes = fs::read(path).map_err(|_| "host profile is not installed")?;
    if bytes.len() > crate::MAX_BYTES {
        return Err("host configuration is too large".into());
    }
    let cfg: HostConfig =
        serde_json::from_slice(&bytes).map_err(|_| "invalid host configuration")?;
    if !cfg.state.is_absolute() {
        return Err("host state path must be absolute".into());
    }
    private_dir(&cfg.state)?;
    Ok(cfg)
}

fn private_dir(path: &Path) -> Result<(), String> {
    // Concurrent first deployments can both reach mkdir before either acquires
    // a scope lock. Existing paths still pass the same privacy/type validation.
    if let Err(error) = fs::DirBuilder::new().mode(0o700).create(path) {
        if error.kind() != std::io::ErrorKind::AlreadyExists {
            return Err("cannot create private state directory".into());
        }
    }
    let meta = fs::symlink_metadata(path).map_err(|_| "cannot inspect state directory")?;
    if !meta.is_dir() || meta.permissions().mode() & 0o077 != 0 {
        return Err("state directory must be private (0700) and not a symlink".into());
    }
    Ok(())
}

use std::os::unix::fs::DirBuilderExt;

fn lock_file(path: &Path) -> Result<File, String> {
    if path
        .symlink_metadata()
        .is_ok_and(|m| m.file_type().is_symlink())
    {
        return Err("lock file must not be a symlink".into());
    }
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(path)
        .map_err(|_| "cannot open deployment lock".into())
}

fn canonical_dir(raw: &str) -> Result<String, String> {
    let path = Path::new(raw);
    if !path.is_absolute() {
        return Err("workspace must be an absolute server path".into());
    }
    let path = path
        .canonicalize()
        .map_err(|_| "workspace does not exist on host")?;
    if !path.is_dir() {
        return Err("workspace is not a directory".into());
    }
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| "workspace is not UTF-8".into())
}

fn executable(raw: &str) -> Result<(), String> {
    let path = Path::new(raw);
    let meta = path
        .metadata()
        .map_err(|_| "profile executable is not installed")?;
    if !path.is_absolute() || !meta.is_file() || meta.permissions().mode() & 0o111 == 0 {
        return Err("profile executable must be an absolute executable file".into());
    }
    Ok(())
}

/// Resolve one immutable launch snapshot using Desktop tiers and host-owned paths.
pub fn snapshot(cfg: &HostConfig, request: &Deploy) -> Result<Snapshot, String> {
    request.provider_config.validate()?;
    let agent = &request.agent;
    if agent
        .provider
        .as_deref()
        .is_some_and(|p| p.trim() == "relay-mesh")
    {
        return Err("relay-mesh agents cannot be deployed through SSH".into());
    }
    let (scope, relay) = wire::identity(agent)?;
    let allowed = cfg
        .relays
        .iter()
        .map(|r| wire::canonical_relay(r))
        .collect::<Result<Vec<_>, _>>()?;
    if !allowed.contains(&relay) {
        return Err("community is not allowed by host configuration".into());
    }
    let profile = cfg
        .profiles
        .get(&request.provider_config.profile)
        .ok_or("unknown installed profile")?;
    if agent.launch.command != profile.command_name {
        return Err("selected runtime does not match host profile".into());
    }
    executable(&profile.harness)?;
    executable(&profile.command)?;
    executable(&profile.cli)?;
    let mut env = agent.launch.policy_env.clone();
    env.extend(agent.launch.env.clone());
    wire::validate_env(&env)?;
    let workspace = canonical_dir(
        env.get("BUZZ_REMOTE_WORKSPACE")
            .unwrap_or(&profile.workspace),
    )?;
    for key in [
        "BUZZ_PRIVATE_KEY",
        "NOSTR_PRIVATE_KEY",
        "BUZZ_AUTH_TAG",
        "BUZZ_API_TOKEN",
        "BUZZ_RELAY_URL",
        "BUZZ_ACP_PRIVATE_KEY",
        "BUZZ_ACP_API_TOKEN",
        "BUZZ_ACP_AGENT_OWNER",
        "BUZZ_ACP_AGENT_COMMAND",
        "BUZZ_ACP_AGENT_ARGS",
        "BUZZ_ACP_RESPOND_TO",
        "BUZZ_ACP_RESPOND_TO_ALLOWLIST",
        "BUZZ_ACP_MCP_COMMAND",
        "BUZZ_MANAGED_AGENT_START_NONCE",
    ] {
        env.remove(key);
    }
    env.extend(profile.env.clone());
    env.insert(
        "BUZZ_PRIVATE_KEY".into(),
        agent.private_key_nsec.trim().into(),
    );
    env.insert(
        "NOSTR_PRIVATE_KEY".into(),
        agent.private_key_nsec.trim().into(),
    );
    env.insert("BUZZ_RELAY_URL".into(), relay);
    let owner = agent
        .launch
        .owner_pubkey
        .as_deref()
        .filter(|s| !s.is_empty());
    if let Some(owner) = owner {
        if owner.len() != 64 || !owner.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err("invalid owner identity".into());
        }
        env.insert("BUZZ_ACP_AGENT_OWNER".into(), owner.into());
    }
    let auth = agent.auth_tag.as_deref().filter(|s| !s.trim().is_empty());
    if owner.is_none() && auth.is_none() {
        return Err("owner identity or authorization is required".into());
    }
    if let Some(auth) = auth {
        env.insert("BUZZ_AUTH_TAG".into(), auth.into());
    }
    if agent
        .launch
        .args
        .iter()
        .any(|a| a.contains(',') || a.contains('\0'))
    {
        return Err("harness arguments cannot contain commas or NUL".into());
    }
    env.insert("BUZZ_ACP_AGENT_COMMAND".into(), profile.command.clone());
    env.insert("BUZZ_ACP_AGENT_ARGS".into(), agent.launch.args.join(","));
    let gate = agent.respond_to.as_deref().unwrap_or("owner-only");
    if !["owner-only", "allowlist", "anyone", "nobody"].contains(&gate)
        || (gate == "allowlist" && agent.respond_to_allowlist.is_empty())
        || agent
            .respond_to_allowlist
            .iter()
            .any(|s| s.len() != 64 || !s.bytes().all(|c| c.is_ascii_hexdigit()))
    {
        return Err("invalid respond-to policy".into());
    }
    env.insert("BUZZ_ACP_RESPOND_TO".into(), gate.into());
    env.insert(
        "BUZZ_ACP_RESPOND_TO_ALLOWLIST".into(),
        agent.respond_to_allowlist.join(","),
    );
    env.insert("BUZZ_ACP_EXIT_AFTER_INACTIVITY".into(), "14400".into());
    env.insert("BUZZ_MANAGED_AGENT".into(), "true".into());
    env.insert(
        "BUZZ_MANAGED_AGENT_START_NONCE".into(),
        uuid::Uuid::new_v4().to_string(),
    );
    env.insert("BUZZ_HOST_SCOPE".into(), scope.clone());
    env.insert(
        "BUZZ_HOST_BINARY".into(),
        std::env::current_exe()
            .map_err(|_| "cannot resolve host executable")?
            .to_str()
            .ok_or("host executable is not UTF-8")?
            .into(),
    );
    wire::validate_env(&env)?;
    Ok(Snapshot {
        scope,
        workspace,
        harness: profile.harness.clone(),
        cli: profile.cli.clone(),
        env,
    })
}

async fn supervisor(args: &[&str]) -> Result<Vec<u8>, String> {
    let mut command = Command::new("systemctl");
    command.arg("--user").args(args);
    process::run(command, b"", Duration::from_secs(30)).await
}

/// Converge concurrent Starts without restarting a running agent or replacing its snapshot.
pub async fn deploy(cfg: &HostConfig, request: Deploy) -> Result<String, String> {
    let snapshot = snapshot(cfg, &request)?;
    let directory = cfg.state.join(&snapshot.scope);
    private_dir(&directory)?;
    let lock = lock_file(&directory.join("deploy.lock"))?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        if lock.try_lock_exclusive().is_ok() {
            break;
        }
        if tokio::time::Instant::now() >= deadline {
            return Err("another deployment is still in progress; retry Start".into());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let unit = format!("buzz-next-agent@{}.service", snapshot.scope);
    let state = supervisor(&["show", "--property=ActiveState", "--value", &unit]).await?;
    match String::from_utf8_lossy(&state).trim() {
        "active" | "activating" | "reloading" => return Ok(snapshot.scope),
        "inactive" | "failed" => {}
        _ => return Err("agent is stopping or supervisor state is unknown; retry later".into()),
    }
    // A separate run lock covers the actual process lifetime, including manual starts.
    let run_lock = lock_file(&directory.join("run.lock"))?;
    run_lock
        .try_lock_exclusive()
        .map_err(|_| "agent process still owns this scope")?;
    let bytes = serde_json::to_vec(&snapshot).map_err(|_| "cannot encode snapshot")?;
    if bytes.len() > crate::MAX_BYTES {
        return Err("resolved launch snapshot exceeds 1 MiB".into());
    }
    let temp = directory.join("launch.tmp");
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&temp)
        .map_err(|_| "cannot create snapshot; inspect incomplete host deployment")?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| "cannot save snapshot")?;
    fs::rename(&temp, directory.join("launch.json")).map_err(|_| "cannot publish snapshot")?;
    drop(run_lock);
    supervisor(&["start", &unit]).await?;
    Ok(snapshot.scope)
}

/// Run a saved snapshot as the systemd-owned process. No shared model lifecycle calls.
pub fn run(cfg: &HostConfig, scope: &str) -> Result<(), String> {
    if scope.len() != 64 || !scope.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("invalid agent scope".into());
    }
    let directory = cfg.state.join(scope);
    private_dir(&directory)?;
    let lock = lock_file(&directory.join("run.lock"))?;
    lock.try_lock_exclusive()
        .map_err(|_| "agent scope is already running")?;
    let path = directory.join("launch.json");
    let meta = fs::symlink_metadata(&path).map_err(|_| "launch snapshot is missing")?;
    if !meta.is_file()
        || meta.permissions().mode() & 0o077 != 0
        || meta.len() > crate::MAX_BYTES as u64
    {
        return Err("launch snapshot is not a private bounded file".into());
    }
    let snapshot: Snapshot =
        serde_json::from_slice(&fs::read(path).map_err(|_| "cannot read launch snapshot")?)
            .map_err(|_| "invalid launch snapshot")?;
    if snapshot.scope != scope {
        return Err("snapshot identity mismatch".into());
    }
    wire::validate_env(&snapshot.env)?;
    // Keep the lock owner alive around the child. File descriptors are CLOEXEC.
    let status = std::process::Command::new(snapshot.harness)
        .current_dir(snapshot.workspace)
        .env_clear()
        .envs(snapshot.env)
        .status()
        .map_err(|_| "cannot start installed harness")?;
    if !status.success() {
        return Err("harness exited unsuccessfully".into());
    }
    Ok(())
}

/// Run the CLI with this agent's saved identity, never the host owner's default key.
pub fn cli(cfg: &HostConfig, scope: &str, args: &[String]) -> Result<i32, String> {
    if scope.len() != 64 || !scope.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("invalid agent scope".into());
    }
    let path = cfg.state.join(scope).join("launch.json");
    let meta = fs::symlink_metadata(&path).map_err(|_| "agent snapshot is missing")?;
    if !meta.is_file()
        || meta.permissions().mode() & 0o077 != 0
        || meta.len() > crate::MAX_BYTES as u64
    {
        return Err("invalid private agent snapshot".into());
    }
    let snapshot: Snapshot =
        serde_json::from_slice(&fs::read(path).map_err(|_| "cannot read snapshot")?)
            .map_err(|_| "cannot decode snapshot")?;
    if snapshot.scope != scope {
        return Err("snapshot identity mismatch".into());
    }
    let status = std::process::Command::new(snapshot.cli)
        .args(args)
        .env_clear()
        .envs(snapshot.env)
        .status()
        .map_err(|_| "cannot run agent CLI")?;
    Ok(status.code().unwrap_or(1))
}
