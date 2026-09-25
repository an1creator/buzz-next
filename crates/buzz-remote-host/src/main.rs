//! Fixed one-shot JSON interface used by SSH Connections.
use buzz_connections::wire::{InspectRequest, HOST_PROTOCOL};
use buzz_remote_host::{config::Config, launch};
use std::{
    io::{Read, Write},
    path::PathBuf,
};

#[tokio::main]
async fn main() {
    #[cfg(unix)]
    if std::env::args().nth(1).as_deref() == Some("run-snapshot") {
        let result = match std::env::args().nth(2) {
            Some(path) => buzz_remote_host::supervisor::run_snapshot(&PathBuf::from(path)).await,
            None => Err("Missing snapshot path".into()),
        };
        if result.is_err() {
            eprintln!("Agent launch failed; inspect the host configuration");
            std::process::exit(1);
        }
        return;
    }
    let response = match respond().await {
        Ok(value) => serde_json::json!({"ok": true, "result": value}),
        Err(error) => serde_json::json!({"ok": false, "error": error}),
    };
    if let Ok(bytes) = serde_json::to_vec(&response) {
        let _ = std::io::stdout().write_all(&bytes);
    }
}

async fn respond() -> Result<serde_json::Value, String> {
    let path = match std::env::var_os("BUZZ_CONNECTIONS_HOST_CONFIG") {
        Some(path) => PathBuf::from(path),
        None => PathBuf::from(std::env::var_os("HOME").ok_or("HOME is missing")?)
            .join(".config/buzz-next/connections-host.json"),
    };
    let config = Config::load(&path)?;
    let mut bytes = zeroize::Zeroizing::new(Vec::new());
    std::io::stdin()
        .take(1_048_577)
        .read_to_end(&mut bytes)
        .map_err(|_| "Cannot read request".to_string())?;
    if bytes.len() > 1_048_576 {
        return Err("Request exceeds its limit".into());
    }
    let raw: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| "Invalid host request".to_string())?;
    if raw.get("op").and_then(|value| value.as_str()) == Some("deploy") {
        let request: launch::Deploy =
            serde_json::from_value(raw).map_err(|_| "Invalid deploy request".to_string())?;
        let snapshot = launch::snapshot(&config, &request)?;
        #[cfg(unix)]
        return serde_json::to_value(
            buzz_remote_host::supervisor::deploy(&config, snapshot).await?,
        )
        .map_err(|_| "Cannot encode launch receipt".into());
        #[cfg(not(unix))]
        {
            let _ = snapshot;
            return Err("This host requires Linux with systemd user services".into());
        }
    }
    let request: InspectRequest =
        serde_json::from_value(raw).map_err(|_| "Unknown host operation".to_string())?;
    match request {
        InspectRequest::Info {} => {
            serde_json::to_value(config.info()?).map_err(|_| "Cannot encode host info".into())
        }
        InspectRequest::ValidateDirectory {
            protocol,
            directory,
        } => {
            if protocol != HOST_PROTOCOL {
                return Err("Host protocol is incompatible".into());
            }
            Ok(serde_json::json!({"path":config.directory(&directory)?}))
        }
        InspectRequest::Status { protocol, scope } => {
            if protocol != HOST_PROTOCOL {
                return Err("Host protocol is incompatible".into());
            }
            #[cfg(unix)]
            return serde_json::to_value(
                buzz_remote_host::supervisor::status(&config, &scope).await?,
            )
            .map_err(|_| "Cannot encode launch status".into());
            #[cfg(not(unix))]
            {
                let _ = scope;
                Err("Unsupported server platform".into())
            }
        }
    }
}
