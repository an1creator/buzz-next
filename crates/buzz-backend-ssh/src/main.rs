//! Desktop-side provider: negotiate, hand off once through SSH, exit.
use buzz_backend_ssh::{process, read_request, reply, wire};
use serde_json::{json, Value};
use std::time::Duration;
use tokio::process::Command;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    reply(respond().await);
}

async fn respond() -> Result<Value, String> {
    let request = read_request().await?;
    match request.get("op").and_then(Value::as_str) {
        Some("info") => Ok(json!({
            "ok":true,"name":"ssh","version":env!("CARGO_PKG_VERSION"),
            "protocol_version":1,
            "description":"Runs an installed autonomous Buzz harness over SSH",
            "config_schema":{"type":"object","required":["host"],"properties":{
                "host":{"type":"string","title":"SSH host or user@host"},
                "profile":{"type":"string","title":"Installed host profile","default":"shared-codex"}
            },"additionalProperties":false}
        })),
        Some("deploy") => {
            let config: wire::ProviderConfig = serde_json::from_value(
                request
                    .get("provider_config")
                    .cloned()
                    .unwrap_or(Value::Null),
            )
            .map_err(|_| "invalid SSH provider configuration")?;
            config.validate()?;
            // No identity crosses the connection before host protocol negotiation.
            let info = ssh(&config.host, "info", b"").await?;
            if info.get("host_protocol") != Some(&json!(1)) || info.get("ok") != Some(&json!(true))
            {
                return Err("host protocol mismatch; install a compatible buzz-host".into());
            }
            let bytes = serde_json::to_vec(&request).map_err(|_| "cannot encode launch request")?;
            ssh(&config.host, "deploy", &bytes).await
        }
        _ => Err("unsupported provider operation".into()),
    }
}

async fn ssh(host: &str, operation: &str, input: &[u8]) -> Result<Value, String> {
    let mut command = Command::new("ssh");
    command.args([
        "-T",
        "-o",
        "BatchMode=yes",
        "-o",
        "StrictHostKeyChecking=yes",
        "-o",
        "ConnectTimeout=15",
        "-o",
        "ServerAliveInterval=15",
        "-o",
        "ServerAliveCountMax=2",
        "--",
        host,
    ]);
    // Only a fixed operation is interpolated; user-controlled text is stdin JSON.
    command.arg(format!("exec \"$HOME/.local/bin/buzz-host\" {operation}"));
    let output = process::run(command, input, Duration::from_secs(90)).await?;
    let response: Value =
        serde_json::from_slice(&output).map_err(|_| "host returned invalid JSON")?;
    if !response.is_object() {
        return Err("host returned a non-object response".into());
    }
    Ok(response)
}
