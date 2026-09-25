//! Saved configuration contains references, never authentication secrets.
use serde::{Deserialize, Serialize};

/// One explicitly added execution location on this Desktop installation.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Connection {
    pub id: String,
    pub name: String,
    pub revision: u64,
    pub target: Target,
    #[serde(default)]
    pub defaults: AgentDefaults,
    #[serde(default)]
    pub check: Option<ConnectionCheck>,
}

/// Transport settings. SSH config remains a live reference, not a flattened copy.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Target {
    Local,
    Ssh { endpoint: SshEndpoint },
}

/// Explicit fields and native config are deliberately distinct modes.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum SshEndpoint {
    Manual {
        host: String,
        port: u16,
        username: String,
        authentication: Authentication,
    },
    Config {
        path: String,
        alias: String,
    },
}

/// Passwords/passphrases are supplied transiently or through OS-keyring references.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "method", rename_all = "snake_case", deny_unknown_fields)]
pub enum Authentication {
    Password {},
    PrivateKey { path: String },
    Agent {},
}

/// Defaults apply to new drafts only, never restart existing agents.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AgentDefaults {
    pub harness_id: Option<String>,
    pub model: Option<String>,
}

/// A revision-scoped observation, not a promise of continuing liveness.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ConnectionCheck {
    pub revision: u64,
    pub checked_at: u64,
    pub outcome: CheckOutcome,
    pub server_id: Option<String>,
    pub default_directory: Option<String>,
}

/// UI states must distinguish failed discovery from a successful empty catalog.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum CheckOutcome {
    Ready,
    Unreachable,
    AuthenticationRequired,
    HostTrustRequired { fingerprint: String },
    HostKeyChanged,
    SetupRequired,
    UpdateRequired { required_protocol: u32 },
    HarnessSetupRequired,
    Failed { message: String },
}

/// Instance-specific configuration; a missing harness is still saveable.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Execution {
    pub connection_id: String,
    pub harness_id: Option<String>,
    pub model: Option<String>,
    #[serde(default)]
    pub directory: WorkingDirectory,
}

/// Automatic resolution is performed on the execution machine.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkingDirectory {
    #[default]
    Automatic,
    Explicit {
        path: String,
    },
}

impl Connection {
    /// Validate persisted input without probing the network or filesystem.
    pub fn validate(&self) -> Result<(), String> {
        uuid::Uuid::parse_str(&self.id).map_err(|_| "Invalid connection ID".to_string())?;
        text(&self.name, "Connection name", 128)?;
        match &self.target {
            Target::Local => (),
            Target::Ssh { endpoint } => match endpoint {
                SshEndpoint::Manual {
                    host,
                    port,
                    username,
                    authentication,
                } => {
                    token(host, "Host")?;
                    token(username, "Username")?;
                    if *port == 0 {
                        return Err("Port must be between 1 and 65535".into());
                    }
                    if let Authentication::PrivateKey { path } = authentication {
                        text(path, "Private key path", 4096)?;
                    }
                }
                SshEndpoint::Config { path, alias } => {
                    text(path, "SSH config path", 4096)?;
                    token(alias, "SSH alias")?;
                    if alias.contains(['*', '?', '!']) {
                        return Err("Choose a concrete SSH alias".into());
                    }
                }
            },
        }
        if let Some(value) = &self.defaults.harness_id {
            text(value, "Harness", 256)?;
        }
        if let Some(value) = &self.defaults.model {
            text(value, "Model", 1024)?;
        }
        Ok(())
    }

    /// A cached observation is usable only for the exact saved revision.
    pub fn checked_ready(&self) -> bool {
        self.check.as_ref().is_some_and(|check| {
            check.revision == self.revision && check.outcome == CheckOutcome::Ready
        })
    }
}

impl Execution {
    /// Save-time validation permits an unconfigured harness but rejects malformed paths.
    pub fn validate(&self, connection: &Connection) -> Result<(), String> {
        if self.connection_id != connection.id {
            return Err("Connection does not match execution".into());
        }
        if let Some(id) = &self.harness_id {
            text(id, "Harness", 256)?;
        }
        if let Some(model) = &self.model {
            text(model, "Model", 1024)?;
        }
        if let WorkingDirectory::Explicit { path } = &self.directory {
            text(path, "Working directory", 4096)?;
            let absolute = match connection.target {
                Target::Local => std::path::Path::new(path).is_absolute(),
                Target::Ssh { .. } => path.starts_with('/'),
            };
            if !absolute {
                return Err("Choose an absolute path on the execution machine".into());
            }
        }
        Ok(())
    }
}

fn text(value: &str, label: &str, limit: usize) -> Result<(), String> {
    if value.trim().is_empty() || value.len() > limit || value.chars().any(char::is_control) {
        return Err(format!(
            "{label} is empty, too long, or contains control characters"
        ));
    }
    Ok(())
}

fn token(value: &str, label: &str) -> Result<(), String> {
    text(value, label, 255)?;
    if value.starts_with('-')
        || value.chars().any(char::is_whitespace)
        || value.contains(['@', '/', '\\'])
    {
        return Err(format!("Invalid {label}"));
    }
    Ok(())
}
