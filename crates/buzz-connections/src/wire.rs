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
