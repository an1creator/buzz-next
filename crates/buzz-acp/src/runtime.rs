//! Shared agent capabilities. Intake and scheduling belong to each entry point;
//! environment, credentials, adapter settings and prompt equipment belong here.
use crate::{
    build_mcp_servers, config::Config, current_working_directory, git, observer, pool, relay,
    resolve_agent_owner, PromptContext,
};
use anyhow::Result;
use std::{collections::HashMap, path::Path, time::Duration};
use uuid::Uuid;

pub(crate) enum SessionMode {
    Conversation,
    Task,
}

/// Own shared launch resources until all adapter work has been drained.
/// Add new runtime capabilities here, not separately to the CLI entry points.
pub(crate) struct AgentRuntime {
    config: Config,
    _git_environment: git::GitEnvironment,
}

impl AgentRuntime {
    pub(crate) fn prepare(mut config: Config) -> Result<Self> {
        let git_environment = git::GitEnvironment::for_config(&mut config)?;
        Ok(Self {
            config,
            _git_environment: git_environment,
        })
    }

    pub(crate) fn config(&self) -> &Config {
        &self.config
    }

    pub(crate) fn startup(&self, observer: Option<observer::ObserverHandle>) -> PoolStartup {
        PoolStartup::from_config(&self.config, observer)
    }

    pub(crate) fn prompt_context(
        &self,
        rest: relay::RestClient,
        channels: HashMap<Uuid, relay::ChannelInfo>,
        mode: SessionMode,
    ) -> Result<PromptContext> {
        make_prompt_context(&self.config, rest, channels, mode)
    }
}

#[derive(Clone)]
pub(crate) struct PoolStartup {
    pub(crate) agents: u32,
    pub(crate) command: String,
    pub(crate) args: Vec<String>,
    pub(crate) extra_env: Vec<(String, String)>,
    pub(crate) has_generated_codex_config: bool,
    pub(crate) model: Option<String>,
    pub(crate) effort_level: Option<String>,
    pub(crate) observer: Option<observer::ObserverHandle>,
}

impl PoolStartup {
    fn from_config(config: &Config, observer: Option<observer::ObserverHandle>) -> Self {
        Self {
            agents: config.agents,
            command: config.agent_command.clone(),
            args: config.agent_args.clone(),
            extra_env: config.persona_env_vars.clone(),
            has_generated_codex_config: config.has_generated_codex_config,
            model: config.model.clone(),
            effort_level: config.effort_level.clone(),
            observer,
        }
    }
}

fn make_prompt_context(
    config: &Config,
    rest_client: relay::RestClient,
    channels: HashMap<Uuid, relay::ChannelInfo>,
    mode: SessionMode,
) -> Result<PromptContext> {
    let base_prompt_content = config.base_prompt_content.as_ref();
    let cwd = current_working_directory()?;
    Ok(PromptContext {
        mcp_servers: build_mcp_servers(config),
        initial_message: config.initial_message.clone(),
        idle_timeout: Duration::from_secs(config.idle_timeout_secs),
        max_turn_duration: Duration::from_secs(config.max_turn_duration_secs),
        turn_liveness_interval: Duration::from_secs(config.turn_liveness_secs),
        dedup_mode: config.dedup_mode,
        system_prompt: config.system_prompt.clone(),
        session_title: config.session_title.clone(),
        team_instructions: config.team_instructions.clone(),
        base_prompt: if config.no_base_prompt {
            None
        } else {
            // Build standing context once under the configured policy, before
            // any session/new. Both modern ACP and legacy first-turn framing
            // consume this same assembled base (including custom base files).
            let base = base_prompt_content
                .map(String::as_str)
                .unwrap_or(include_str!("base_prompt.md"));
            let base = with_cli_location(
                base,
                std::env::var("BUZZ_CLI_BINARY").ok().as_deref(),
                std::env::var_os("BUZZ_CODEX_SOCKET").is_some(),
            );
            Some(if matches!(mode, SessionMode::Task) {
                format!("{base}\n\n{}", include_str!("session_model_task.md"))
            } else {
                config.session_policy.append_session_model(&base)
            })
        },
        heartbeat_prompt: config.heartbeat_prompt.clone(),
        cwd,
        rest_client: rest_client.clone(),
        channel_info: pool::ChannelInfoResolver::new(channels, rest_client),
        context_message_limit: config.context_message_limit,
        max_turns_per_session: config.max_turns_per_session,
        permission_mode: config.permission_mode,
        agent_keys: config.keys.clone(),
        agent_owner_pubkey: resolve_agent_owner(config)
            .as_deref()
            .and_then(|hex| nostr::PublicKey::from_hex(hex).ok()),
        memory_enabled: config.memory_enabled,
        harness_name: crate::config::normalize_agent_command_identity(&config.agent_command),
        relay_url: config.relay_url.clone(),
    })
}

fn with_cli_location(base: &str, cli_binary: Option<&str>, shared_codex: bool) -> String {
    let Some(path) =
        cli_binary.filter(|path| Path::new(path).is_absolute() && !path.contains(['\n', '\r']))
    else {
        return base.to_owned();
    };
    let quoted = format!("'{}'", path.replace('\'', "'\\''"));
    let mut prompt = format!(
        "{base}\n\nOn this connection, the Buzz CLI executable is {quoted}. \
         Use this absolute executable for every Buzz CLI command, including in \
         login shells that reset PATH. The examples above use `buzz` as shorthand."
    );
    if shared_codex {
        prompt.push_str(
            "\nFor Buzz CLI commands, use the Buzz developer MCP shell tool. It has this \
             agent's signing identity. The shared Codex App Server's built-in shell \
             does not have that identity and cannot send replies, even when it can \
             find the `buzz` executable.",
        );
    }
    prompt
}

#[cfg(test)]
mod cli_location_tests {
    use super::with_cli_location;

    #[test]
    fn remote_cli_location_survives_login_shell_and_ignores_invalid_paths() {
        let prompt = with_cli_location("base", Some("/opt/buzz next/buzz"), true);
        assert!(prompt.contains("'/opt/buzz next/buzz'"));
        assert!(prompt.contains("login shells that reset PATH"));
        assert!(prompt.contains("Buzz developer MCP shell tool"));
        assert_eq!(with_cli_location("base", None, false), "base");
        assert_eq!(
            with_cli_location("base", Some("relative/buzz"), false),
            "base"
        );
        assert_eq!(
            with_cli_location("base", Some("/tmp/buzz\ninjected"), false),
            "base"
        );
    }
}
