//! Short-lived ACP model discovery on the execution host; never connects to relay.
use crate::config::{executable, Config};
use buzz_connections::model::WorkingDirectory;
use std::time::Duration;

/// Inspect the configured harness using bounded output and process-tree containment.
pub async fn discover(config: &Config, harness_id: &str) -> Result<serde_json::Value, String> {
    let harness = config
        .harnesses
        .iter()
        .find(|item| item.catalog.id == harness_id)
        .ok_or("Harness is no longer available on this server")?;
    executable(&config.acp_binary)?;
    executable(&harness.executable)?;
    if harness.args.iter().any(|arg| arg.contains(',')) {
        return Err("Harness arguments cannot contain commas in this ACP version".into());
    }
    let mut command = tokio::process::Command::new(&config.acp_binary);
    command
        .args(["models", "--json"])
        .current_dir(config.directory(&WorkingDirectory::Automatic)?)
        .envs(&harness.environment)
        .env("BUZZ_ACP_AGENT_COMMAND", &harness.executable)
        .env("BUZZ_ACP_AGENT_ARGS", harness.args.join(","));
    let output = buzz_connections::process::run(command, b"", Duration::from_secs(30)).await?;
    if !output.success {
        // Tool stderr can contain credentials. Keep it out of the Desktop response.
        return Err(
            "Model discovery failed. Check the harness installation and sign-in on this server."
                .into(),
        );
    }
    serde_json::from_slice(&output.stdout).map_err(|_| "Harness returned invalid model data".into())
}
