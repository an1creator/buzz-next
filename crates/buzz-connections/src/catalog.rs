//! Canonical serializable harness catalog shared by local and server discovery.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AcpAvailabilityStatus {
    Available,
    AdapterMissing,
    /// Adapter binary is present but unsupported — either the deprecated
    /// package or a version below the supported floor. Reinstall required.
    AdapterOutdated,
    CliMissing,
    NotInstalled,
}

/// Authentication/login status for a CLI-based ACP runtime. Serializes as a tagged union
/// `{ status: "...", diagnostic?: "..." }` so the TypeScript side can exhaustively switch on `status`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum AuthStatus {
    /// The CLI reported a successful login.
    LoggedIn,
    /// The CLI exited non-zero without a config-parse signal.
    LoggedOut,
    /// The CLI exited non-zero and its stderr contains a config-parse error.
    ConfigInvalid {
        /// Trimmed excerpt of the stderr message.
        diagnostic: String,
    },
    /// This runtime does not have a login step (e.g. goose, buzz-agent).
    NotApplicable,
    /// Probe was not attempted (runtime unavailable or probe timed out).
    Unknown,
}

/// Origin of an ACP runtime catalog entry. Serializes as a lowercase string so the TypeScript consumer can switch on it without numeric comparisons.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HarnessSource {
    /// Compiled into the app — one of the four first-class runtimes.
    Builtin,
    /// Static preset entry with bundled logo, PATH-probed, not editable/deletable.
    Preset,
    /// Loaded at runtime from the user's `custom_harnesses/` directory.
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcpRuntimeCatalogEntry {
    pub id: String,
    pub label: String,
    pub avatar_url: String,
    pub availability: AcpAvailabilityStatus,
    pub command: Option<String>,
    pub binary_path: Option<String>,
    pub default_args: Vec<String>,
    pub mcp_command: Option<String>,
    /// Environment variable used to apply the initial model, when supported.
    pub model_env_var: Option<String>,
    /// Environment variable used to apply the selected LLM provider, when supported.
    pub provider_env_var: Option<String>,
    /// Environment variable used to apply thinking effort, when supported.
    pub thinking_env_var: Option<String>,
    /// Canonical accepted effort values for this runtime, in display order.
    /// Serialized from `KnownAcpRuntime::effort_normalization.canonical` for
    /// runtimes with a static finite vocabulary (e.g. Goose). `None` for
    /// runtimes with no canonicalization contract (buzz-agent uses a
    /// provider/model catalog; Claude/Codex/unknown runtimes accept any string).
    ///
    /// The renderer uses this to drive choices and validation, replacing the
    /// TS-side `GOOSE_EFFORT_CANONICAL_VALUES` duplicate. When non-null, the
    /// `harnessNative` effort field uses this list exclusively — `off` and all
    /// other valid Goose values are always present when this is Goose, so
    /// `useEffortAutoClear` never incorrectly deletes a valid saved value.
    pub effort_canonical_values: Option<Vec<String>>,
    pub max_tokens_env_var: Option<String>,
    pub context_limit_env_var: Option<String>,
    pub max_rounds_env_var: Option<String>,
    pub install_hint: String,
    pub install_instructions_url: String,
    /// true when at least one automated install step is available
    pub can_auto_install: bool,
    /// true when this runtime depends on a separately installed vendor CLI.
    pub requires_external_cli: bool,
    pub underlying_cli_path: Option<String>,
    /// true when an npm adapter step is pending but Node.js / npm is absent.
    /// The UI hides the Install button and shows a Node.js install callout.
    pub node_required: bool,
    /// Login/authentication status for CLI-based runtimes.
    pub auth_status: AuthStatus,
    /// Hint for completing authentication, shown when `auth_status` is not `logged_in`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub login_hint: Option<String>,
    /// Whether this entry came from the compiled-in catalog or a user-supplied
    /// JSON file in `custom_harnesses/`. The UI uses this to decide editability.
    pub source: HarnessSource,
    /// Definition-level env vars for `source: custom` entries; populated from
    /// `HarnessDefinition.env` so saves don't silently erase existing vars.
    /// Absent for builtin/preset entries. Skipped when empty in serialization.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub definition_env: BTreeMap<String, String>,
    /// Spawn-time parallelism cap; absent for uncapped harnesses.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_parallelism: Option<u32>,
}
