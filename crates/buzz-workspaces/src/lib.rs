//! One working-folder contract for Desktop, remote deployment and ACP sessions.
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};
use uuid::Uuid;

/// Agent-local default; an empty value inherits the execution target's default.
pub const DEFAULT_ENV: &str = "BUZZ_ACP_WORKSPACE";
/// Launcher-owned channel UUID to absolute folder mapping, scoped before launch.
pub const CHANNELS_ENV: &str = "BUZZ_ACP_CHANNEL_WORKSPACES";
/// Maximum channel overrides in one launch snapshot.
pub const MAX_CHANNELS: usize = 256;
/// Maximum serialized channel mapping size.
pub const MAX_CONFIG_BYTES: usize = 65_536;

/// Authored paths; physical validation belongs to the machine that executes them.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Workspaces {
    /// Agent default, falling back to the execution target when absent.
    pub default: Option<String>,
    /// Overrides for channels in this deployment's community only.
    pub channels: BTreeMap<Uuid, String>,
}

/// Reject invalid path text without interpreting a server path on the client OS.
pub fn validate_path_text(path: &str) -> Result<(), String> {
    if path.trim().is_empty() || path.len() > 4096 || path.chars().any(char::is_control) {
        return Err(
            "Enter a working folder of at most 4096 bytes without control characters".into(),
        );
    }
    let bytes = path.as_bytes();
    let windows_drive = bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'/' | b'\\');
    if !path.starts_with('/') && !path.starts_with("\\\\") && !windows_drive {
        return Err("Use an absolute folder path on the computer where the agent runs".into());
    }
    Ok(())
}

impl Workspaces {
    /// Decode bounded launch values without reading process-global environment.
    pub fn parse(default: Option<&str>, channels: Option<&str>) -> Result<Self, String> {
        let default = default.filter(|v| !v.trim().is_empty()).map(str::to_owned);
        if let Some(path) = &default {
            validate_path_text(path)?;
        }
        let channels = match channels.filter(|v| !v.is_empty()) {
            Some(raw) if raw.len() <= MAX_CONFIG_BYTES => serde_json::from_str(raw)
                .map_err(|_| "Channel working folders must map channel UUIDs to absolute paths")?,
            Some(_) => {
                return Err("Channel working folders exceed the configuration size limit".into())
            }
            None => BTreeMap::new(),
        };
        let value = Self { default, channels };
        value.validate()?;
        Ok(value)
    }

    /// Validate an authored configuration at every persistence/launch boundary.
    pub fn validate(&self) -> Result<(), String> {
        if let Some(path) = &self.default {
            validate_path_text(path)?;
        }
        if self.channels.len() > MAX_CHANNELS {
            return Err(format!(
                "At most {MAX_CHANNELS} channel working folders are supported"
            ));
        }
        for path in self.channels.values() {
            validate_path_text(path)?;
        }
        if serde_json::to_vec(&self.channels)
            .map_err(|_| "Cannot encode working folders")?
            .len()
            > MAX_CONFIG_BYTES
        {
            return Err("Channel working folders exceed the configuration size limit".into());
        }
        Ok(())
    }

    /// Pin existing canonical directories on the execution machine before use.
    pub fn resolve(&self, fallback: &Path) -> Result<ResolvedWorkspaces, String> {
        self.validate()?;
        let default = canonical_folder(self.default.as_deref().map(Path::new).unwrap_or(fallback))?;
        let channels = self
            .channels
            .iter()
            .map(|(channel, path)| {
                canonical_folder(Path::new(path))
                    .map(|path| (*channel, path))
                    .map_err(|error| format!("Working folder for channel {channel}: {error}"))
            })
            .collect::<Result<_, _>>()?;
        Ok(ResolvedWorkspaces { default, channels })
    }
}

/// Canonical paths frozen for a running harness; edits require a new launch.
#[derive(Debug, Clone)]
pub struct ResolvedWorkspaces {
    /// Resolved agent/target default.
    pub default: String,
    /// Resolved channel overrides.
    pub channels: BTreeMap<Uuid, String>,
}

/// Validate an existing absolute directory on this execution machine.
pub fn canonical_folder(path: &Path) -> Result<String, String> {
    if !path.is_absolute() {
        return Err("Working folder must be an absolute path on the execution machine".into());
    }
    let canonical = path
        .canonicalize()
        .map_err(|_| format!("Folder does not exist: {}", path.display()))?;
    if !canonical.is_dir() {
        return Err(format!("Not a folder: {}", path.display()));
    }
    canonical
        .to_str()
        .map(str::to_owned)
        .ok_or_else(|| "Working folder is not UTF-8".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_accepts_absolute_paths_without_interpreting_the_server_os() {
        for value in ["/srv/work", "C:\\work", "\\\\server\\share"] {
            assert!(validate_path_text(value).is_ok());
        }
        for value in ["", "relative", "~/work", "C:work", "/srv/\nwork"] {
            assert!(validate_path_text(value).is_err());
        }
    }

    #[test]
    fn resolution_pins_channel_and_agent_directories() {
        let root = tempfile::tempdir().unwrap();
        let agent = root.path().join("agent");
        let channel = root.path().join("channel");
        std::fs::create_dir(&agent).unwrap();
        std::fs::create_dir(&channel).unwrap();
        let id = Uuid::new_v4();
        let configured = Workspaces {
            default: Some(agent.to_str().unwrap().into()),
            channels: [(id, channel.to_str().unwrap().into())].into(),
        };
        let resolved = configured.resolve(root.path()).unwrap();
        assert_eq!(Path::new(&resolved.default), agent.canonicalize().unwrap());
        assert_eq!(
            Path::new(&resolved.channels[&id]),
            channel.canonicalize().unwrap()
        );
        std::fs::remove_dir(channel).unwrap();
        assert!(configured.resolve(root.path()).is_err());
    }

    #[test]
    fn empty_default_inherits_but_invalid_overrides_never_fall_back() {
        let root = tempfile::tempdir().unwrap();
        let parsed = Workspaces::parse(Some(""), None).unwrap();
        assert_eq!(
            parsed.resolve(root.path()).unwrap().default,
            root.path().to_str().unwrap()
        );
        assert!(Workspaces::parse(None, Some(r#"{"not-a-channel":"/tmp"}"#)).is_err());
        assert!(Workspaces::parse(None, Some(&"x".repeat(MAX_CONFIG_BYTES + 1))).is_err());
        assert!(Workspaces::parse(Some("relative"), None).is_err());
    }
}
