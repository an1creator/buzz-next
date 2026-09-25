//! Versioned host protocol, independent of the unchanged provider-v1 envelope.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Negotiated before a caller sends any identity or model credentials.
pub const HOST_PROTOCOL: u32 = 1;

/// Stable deployment identity; local connection aliases do not enter the scope.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DeploymentScope {
    pub server_id: String,
    pub owner_pubkey: String,
    pub relay_url: String,
    pub agent_pubkey: String,
}

impl DeploymentScope {
    /// A deterministic, filename-safe scope. Length framing prevents concatenation collisions.
    pub fn key(&self) -> String {
        let mut hash = Sha256::new();
        for value in [
            &self.server_id,
            &self.owner_pubkey,
            &self.relay_url,
            &self.agent_pubkey,
        ] {
            hash.update((value.len() as u64).to_be_bytes());
            hash.update(value.as_bytes());
        }
        hex::encode(hash.finalize())
    }
}

/// A receipt proves handoff, not conversational availability.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LaunchReceipt {
    pub request_id: String,
    pub scope: DeploymentScope,
    pub generation: String,
    pub directory: String,
    pub harness_id: String,
    pub state: LaunchState,
}

/// Only relay presence may promote a handoff to Available in the UI.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LaunchState {
    Starting,
    Running,
    Stopped,
    Failed,
    Unconfirmed,
}

/// Read-only server response. No credentials or harness environment are returned.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HostInfo {
    pub protocol: u32,
    pub server_id: String,
    pub version: String,
    pub default_directory: String,
    pub harnesses: Vec<crate::catalog::AcpRuntimeCatalogEntry>,
}

/// Secret-free operations use the same versioned stdin channel as deploy.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum InspectRequest {
    Info {},
    Models {
        protocol: u32,
        harness_id: String,
    },
    ValidateDirectory {
        protocol: u32,
        directory: crate::model::WorkingDirectory,
    },
    Status {
        protocol: u32,
        scope: DeploymentScope,
    },
}

/// Resolved directory is computed by the execution machine, never the launcher.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DirectoryInfo {
    pub path: String,
}

/// Canonical relay scope shared by Desktop and the execution host.
pub fn canonical_relay(raw: &str) -> Result<String, String> {
    let mut url = url::Url::parse(raw.trim()).map_err(|_| "Invalid relay URL".to_string())?;
    if !matches!(url.scheme(), "ws" | "wss")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(
            "Relay must be a ws/wss endpoint without credentials, query or fragment".into(),
        );
    }
    if url.path() == "/" {
        url.set_path("");
    }
    Ok(url.to_string().trim_end_matches('/').to_string())
}
