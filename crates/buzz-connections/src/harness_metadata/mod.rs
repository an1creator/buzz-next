//! Backend-owned harness capabilities used by both Desktop and server discovery.
mod types;
pub use types::*;
#[macro_use]
mod windows_install;
mod catalog;
pub use catalog::KNOWN_ACP_RUNTIMES;
pub const GOOSE_AVATAR_URL: &str = "https://goose-docs.ai/img/logo_dark.png";
pub const CLAUDE_CODE_AVATAR_URL: &str = "https://anthropic.gallerycdn.vsassets.io/extensions/anthropic/claude-code/2.1.77/1773707456892/Microsoft.VisualStudio.Services.Icons.Default";
pub const CODEX_AVATAR_URL: &str = "https://openai.gallerycdn.vsassets.io/extensions/openai/chatgpt/26.5313.41514/1773706730621/Microsoft.VisualStudio.Services.Icons.Default";
pub const BUZZ_AGENT_AVATAR_URL: &str =
    "https://raw.githubusercontent.com/block/buzz/refs/heads/main/crates/buzz-agent/buzz-agent.png";

/// Look up a canonical harness ID without probing commands or running processes.
pub fn by_id(id: &str) -> Option<&'static KnownAcpRuntime> {
    KNOWN_ACP_RUNTIMES.iter().find(|runtime| runtime.id == id)
}
