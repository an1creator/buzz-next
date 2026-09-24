//! Validated launch contract. Identity is derived from the supplied key.
use nostr::{FromBech32, Keys, SecretKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Non-secret, Desktop-persisted provider settings.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderConfig {
    /// OpenSSH config alias or user@host; never shell syntax.
    pub host: String,
    /// An operator-installed host profile, independent of Desktop paths.
    #[serde(default = "default_profile")]
    pub profile: String,
}
fn default_profile() -> String {
    "shared-codex".into()
}
impl ProviderConfig {
    /// Reject option injection and shell metacharacters before spawning SSH.
    pub fn validate(&self) -> Result<(), String> {
        if self.host.is_empty()
            || self.host.len() > 253
            || self.host.starts_with('-')
            || !self
                .host
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"._-@".contains(&c))
            || self.host.matches('@').count() > 1
            || !self
                .host
                .split('@')
                .all(|p| p.as_bytes().first().is_some_and(u8::is_ascii_alphanumeric))
        {
            return Err("host must be an SSH alias or user@host without options".into());
        }
        if self.profile.is_empty()
            || self.profile.len() > 64
            || !self
                .profile
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
        {
            return Err("invalid installed profile name".into());
        }
        Ok(())
    }
}

/// Secret-bearing deploy request; deliberately not Debug.
#[derive(Deserialize)]
pub struct Deploy {
    /// Agent key and resolved launch settings.
    pub agent: Agent,
    /// Non-secret host selection.
    pub provider_config: ProviderConfig,
}
/// Secret-bearing identity and policy; never format this object into logs.
#[derive(Deserialize)]
pub struct Agent {
    /// Community endpoint.
    pub relay_url: String,
    /// Owner-created agent key.
    pub private_key_nsec: String,
    /// Optional identity assertion; it must match the derived identity.
    pub pubkey: Option<String>,
    /// Owner authorization envelope.
    pub auth_tag: Option<String>,
    /// Resolved model provider; relay-mesh cannot run on this substrate.
    pub provider: Option<String>,
    /// Desktop-resolved launch block is mandatory.
    pub launch: Launch,
    /// Harness response gate.
    pub respond_to: Option<String>,
    /// Allowed requesting identities.
    #[serde(default)]
    pub respond_to_allowlist: Vec<String>,
}
/// Same resolved launch tiers consumed by the upstream Kubernetes provider.
#[derive(Deserialize)]
pub struct Launch {
    /// Logical command name mapped by the installed host profile.
    pub command: String,
    /// Arguments for the selected ACP agent.
    #[serde(default)]
    pub args: Vec<String>,
    /// Resolved behavior defaults.
    #[serde(default)]
    pub policy_env: BTreeMap<String, String>,
    /// Resolved user environment.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Owner identity needed for terminal shutdown.
    pub owner_pubkey: Option<String>,
}

/// Private, immutable-at-runtime snapshot loaded by the host supervisor.
#[derive(Serialize, Deserialize)]
pub struct Snapshot {
    /// Stable deployment identity: community + agent public key.
    pub scope: String,
    /// Canonical working directory on the server.
    pub workspace: String,
    /// Absolute installed harness path.
    pub harness: String,
    /// Installed CLI used for agent-signed relay operations.
    pub cli: String,
    /// Entire environment, including credentials, stored with mode 0600.
    pub env: BTreeMap<String, String>,
}

/// Derive the deployment scope rather than trusting a caller's public key.
pub fn identity(agent: &Agent) -> Result<(String, String), String> {
    let secret =
        SecretKey::from_bech32(agent.private_key_nsec.trim()).map_err(|_| "invalid agent nsec")?;
    let pubkey = Keys::new(secret).public_key().to_hex();
    if agent
        .pubkey
        .as_ref()
        .is_some_and(|claimed| claimed != &pubkey)
    {
        return Err("agent identity assertion does not match key".into());
    }
    let relay = canonical_relay(&agent.relay_url)?;
    let mut hash = Sha256::new();
    hash.update(relay.as_bytes());
    hash.update([0]);
    hash.update(pubkey.as_bytes());
    Ok((hex::encode(hash.finalize()), relay))
}

/// Normalize community URLs before selecting a deployment identity.
pub fn canonical_relay(raw: &str) -> Result<String, String> {
    let mut url = url::Url::parse(raw.trim()).map_err(|_| "invalid relay URL")?;
    if url.scheme() != "wss"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("relay must be a credential-free wss URL".into());
    }
    if url.path().is_empty() {
        url.set_path("/");
    }
    Ok(url.to_string())
}

/// Validate the environment before it crosses an OS process boundary.
pub fn validate_env(env: &BTreeMap<String, String>) -> Result<(), String> {
    for (key, value) in env {
        let mut chars = key.chars();
        if !chars
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
            || value.contains('\0')
        {
            return Err("invalid launch environment".into());
        }
        if key.eq_ignore_ascii_case("BUZZ_ACP_NO_PRESENCE") {
            return Err("remote agents require presence".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nostr::ToBech32;

    fn agent() -> Agent {
        let keys = Keys::parse("0000000000000000000000000000000000000000000000000000000000000001")
            .unwrap();
        serde_json::from_value(serde_json::json!({
            "relay_url":"wss://example.com", "private_key_nsec":keys.secret_key().to_bech32().unwrap(),
            "launch":{"command":"codex-acp"}
        })).unwrap()
    }
    #[test]
    fn scope_is_community_bound_and_normalized() {
        let mut a = agent();
        let first = identity(&a).unwrap().0;
        a.relay_url = "wss://EXAMPLE.COM/".into();
        assert_eq!(first, identity(&a).unwrap().0);
        a.relay_url = "wss://other.example/".into();
        assert_ne!(first, identity(&a).unwrap().0);
        a.pubkey = Some("a".repeat(64));
        assert!(identity(&a).is_err());
    }
    #[test]
    fn ssh_rejects_options_and_shell_input() {
        for host in [
            "-oProxyCommand=x",
            "host;id",
            "host\nother",
            "user@-host",
            "",
            "host $(id)",
        ] {
            assert!(ProviderConfig {
                host: host.into(),
                profile: default_profile()
            }
            .validate()
            .is_err());
        }
        assert!(ProviderConfig {
            host: "user@MyServer".into(),
            profile: default_profile()
        }
        .validate()
        .is_ok());
    }
    #[test]
    fn presence_cannot_be_disabled() {
        assert!(validate_env(&BTreeMap::from([(
            "buzz_acp_no_presence".into(),
            "1".into()
        )]))
        .is_err());
    }
}
