//! Operator-owned locations and backend catalog; discovery never creates state.
use buzz_connections::{
    catalog::{AcpAvailabilityStatus, AcpRuntimeCatalogEntry},
    model::WorkingDirectory,
    wire::{HostInfo, HOST_PROTOCOL},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    io::Read,
    path::{Path, PathBuf},
};

/// Private operator configuration, separate from per-agent credentials.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub server_id: String,
    pub state_directory: PathBuf,
    pub default_directory: String,
    pub relay_urls: Vec<String>,
    pub acp_binary: PathBuf,
    pub cli_binary: PathBuf,
    pub harnesses: Vec<Harness>,
}

/// Explicit installed harness variant; UI labels and capability metadata come from here.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Harness {
    pub catalog: AcpRuntimeCatalogEntry,
    pub executable: PathBuf,
    /// Canonical capability source for a builtin variant; absent for custom profiles.
    #[serde(default)]
    pub runtime_id: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
}

impl Config {
    /// Bound input size and avoid reflecting config content in errors.
    pub fn load(path: &Path) -> Result<Self, String> {
        let file = std::fs::File::open(path).map_err(|_| "Server setup is required".to_string())?;
        let mut bytes = zeroize::Zeroizing::new(Vec::new());
        file.take(1_048_577)
            .read_to_end(&mut bytes)
            .map_err(|_| "Cannot read host configuration".to_string())?;
        if bytes.len() > 1_048_576 {
            return Err("Host configuration exceeds its limit".into());
        }
        let config: Self = serde_json::from_slice(&bytes)
            .map_err(|_| "Host configuration is invalid".to_string())?;
        uuid::Uuid::parse_str(&config.server_id)
            .map_err(|_| "Host server_id must be a UUID".to_string())?;
        if !config.state_directory.is_absolute()
            || !config.acp_binary.is_absolute()
            || !config.cli_binary.is_absolute()
        {
            return Err("Host paths must be absolute".into());
        }
        let mut ids = std::collections::BTreeSet::new();
        for harness in &config.harnesses {
            if harness.catalog.id.is_empty() || !ids.insert(&harness.catalog.id) {
                return Err("Host harness IDs must be unique and nonempty".into());
            }
        }
        Ok(config)
    }

    /// No filesystem writes, installation, model invocation, or agent start.
    pub fn info(&self) -> Result<HostInfo, String> {
        let mut harnesses = Vec::new();
        for harness in &self.harnesses {
            let mut catalog = harness.catalog.clone();
            catalog.definition_env.clear();
            // Operator arguments may contain credentials; only the host consumes them.
            catalog.default_args.clear();
            catalog.can_auto_install = false;
            if executable(&harness.executable).is_err()
                || executable(&self.acp_binary).is_err()
                || executable(&self.cli_binary).is_err()
                || self.mcp_binary(harness).is_err()
            {
                catalog.availability = AcpAvailabilityStatus::AdapterMissing;
            }
            harnesses.push(catalog);
        }
        Ok(HostInfo {
            protocol: HOST_PROTOCOL,
            server_id: self.server_id.clone(),
            version: env!("CARGO_PKG_VERSION").into(),
            default_directory: self.directory(&WorkingDirectory::Automatic)?,
            harnesses,
        })
    }

    /// Resolve tools from the operator bundle, never from the agent's PATH.
    pub fn mcp_binary(&self, harness: &Harness) -> Result<Option<PathBuf>, String> {
        let Some(command) = harness
            .catalog
            .mcp_command
            .as_deref()
            .filter(|s| !s.is_empty())
        else {
            return Ok(None);
        };
        let path = Path::new(command);
        let path = if path.is_absolute() {
            path.to_owned()
        } else if path.components().count() == 1 {
            self.acp_binary
                .parent()
                .ok_or("Invalid ACP bundle path")?
                .join(path)
        } else {
            return Err("MCP command must be a bundle executable or an absolute path".into());
        };
        executable(&path)?;
        Ok(Some(path))
    }

    /// Read-only bounded authentication probes; no credential files are read by the host.
    pub async fn inspect(&self) -> Result<HostInfo, String> {
        use buzz_connections::{catalog::AuthStatus, harness_metadata};
        let mut info = self.info()?;
        for (harness, catalog) in self.harnesses.iter().zip(&mut info.harnesses) {
            let metadata = harness
                .runtime_id
                .as_deref()
                .and_then(harness_metadata::by_id);
            if let Some(metadata) = metadata {
                if let Some(probe) = metadata.auth_probe_args {
                    catalog.auth_status = AuthStatus::Unknown;
                    if let Some((binary, args)) = probe.split_first() {
                        if let Some(binary) = crate::setup::find_executable(binary) {
                            let mut command = tokio::process::Command::new(binary);
                            command.args(args).envs(&harness.environment);
                            if let Ok(output) = buzz_connections::process::run(
                                command,
                                b"",
                                std::time::Duration::from_secs(8),
                            )
                            .await
                            {
                                catalog.auth_status = if output.success {
                                    AuthStatus::LoggedIn
                                } else {
                                    AuthStatus::LoggedOut
                                };
                            }
                        }
                    }
                }
            }
        }
        Ok(info)
    }

    /// Resolve on the host. Explicit invalid paths never fall back or create directories.
    pub fn directory(&self, selection: &WorkingDirectory) -> Result<String, String> {
        let raw = match selection {
            WorkingDirectory::Automatic => &self.default_directory,
            WorkingDirectory::Explicit { path } => path,
        };
        let path = Path::new(raw);
        if !path.is_absolute() {
            return Err("Working directory must be an absolute server path".into());
        }
        let canonical = path
            .canonicalize()
            .map_err(|_| "Working directory does not exist or is inaccessible".to_string())?;
        if !canonical.is_dir() {
            return Err("Working directory is not a directory".into());
        }
        std::fs::read_dir(&canonical)
            .map_err(|_| "Working directory is inaccessible".to_string())?;
        canonical
            .to_str()
            .map(str::to_owned)
            .ok_or_else(|| "Working directory is not valid UTF-8".into())
    }
}

/// Only absolute installed executables may enter the immutable launch snapshot.
pub fn executable(path: &Path) -> Result<(), String> {
    let meta = path
        .metadata()
        .map_err(|_| "A required server executable is missing".to_string())?;
    if !path.is_absolute() || !meta.is_file() {
        return Err("Server executable must be an absolute file".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o111 == 0 {
            return Err("Server executable is not executable".into());
        }
    }
    Ok(())
}
