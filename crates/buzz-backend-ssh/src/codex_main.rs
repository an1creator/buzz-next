//! ACP framing and a socket-only transport for the operator's shared App Server.
//! The pinned official codex-acp package owns ACP/Codex protocol translation.
#[cfg(unix)]
mod linux {
    use futures_util::{SinkExt, StreamExt};
    use serde_json::{json, Value};
    use std::{collections::BTreeMap, process::Stdio, time::Duration};
    use tokio::{io::AsyncWriteExt, process::Command};
    use tokio_util::codec::{FramedRead, LinesCodec};

    const FRAME_LIMIT: usize = 16 * 1024 * 1024;

    pub async fn run() -> Result<(), String> {
        match std::env::args().nth(1).as_deref() {
            Some("app-server") => transport().await,
            None => acp().await,
            _ => Err("only ACP stdio or app-server socket transport is supported".into()),
        }
    }

    async fn transport() -> Result<(), String> {
        let path = std::env::var("BUZZ_CODEX_SOCKET")
            .map_err(|_| "shared Codex socket is not configured")?;
        let socket = tokio::time::timeout(
            Duration::from_secs(15),
            tokio::net::UnixStream::connect(path),
        )
        .await
        .map_err(|_| "shared socket connection timed out")?
        .map_err(|_| "cannot connect to shared socket")?;
        let (mut socket, _) = tokio::time::timeout(
            Duration::from_secs(15),
            tokio_tungstenite::client_async("ws://localhost/", socket),
        )
        .await
        .map_err(|_| "shared socket handshake timed out")?
        .map_err(|_| "shared socket handshake failed")?;
        let mut input = FramedRead::new(
            tokio::io::stdin(),
            LinesCodec::new_with_max_length(FRAME_LIMIT),
        );
        let mut output = tokio::io::stdout();
        loop {
            tokio::select! {
                line = input.next() => match line {
                    Some(Ok(line)) => {
                        let _: Value = serde_json::from_str(&line).map_err(|_| "invalid JSON from adapter")?;
                        socket.send(line.into()).await.map_err(|_| "shared socket send failed")?;
                    },
                    None => { let _ = socket.close(None).await; return Ok(()); },
                    _ => return Err("adapter frame limit exceeded".into()),
                },
                message = socket.next() => match message {
                    Some(Ok(tokio_tungstenite::tungstenite::Message::Text(text))) => {
                        if text.len() > FRAME_LIMIT { return Err("shared socket frame limit exceeded".into()); }
                        output.write_all(text.as_bytes()).await.map_err(|_| "adapter closed output")?;
                        output.write_all(b"\n").await.map_err(|_| "adapter closed output")?;
                        output.flush().await.map_err(|_| "adapter closed output")?;
                    },
                    Some(Ok(tokio_tungstenite::tungstenite::Message::Ping(data))) => {
                        socket.send(tokio_tungstenite::tungstenite::Message::Pong(data)).await.map_err(|_| "socket ping failed")?;
                    },
                    Some(Ok(tokio_tungstenite::tungstenite::Message::Close(_))) | None => return Ok(()),
                    Some(Err(_)) => return Err("shared socket closed unexpectedly".into()),
                    _ => {},
                }
            }
        }
    }

    async fn acp() -> Result<(), String> {
        let entry = std::env::var("BUZZ_CODEX_ACP_ENTRY")
            .map_err(|_| "official ACP package is not installed")?;
        let exe =
            std::env::current_exe().map_err(|_| "cannot resolve socket transport executable")?;
        let scope = std::env::var("BUZZ_HOST_SCOPE").map_err(|_| "agent scope is missing")?;
        let host = std::env::var("BUZZ_HOST_BINARY").map_err(|_| "agent CLI binding is missing")?;
        let mut child = Command::new("node")
            .arg(entry)
            .env("CODEX_PATH", exe)
            .env("INITIAL_AGENT_MODE", "agent-full-access")
            .env_remove("APP_SERVER_LOGS")
            .env_remove("CODEX_CONFIG")
            .env_remove("DEFAULT_AUTH_REQUEST")
            .env_remove("MODEL_PROVIDER")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| "cannot launch pinned ACP adapter")?;
        let mut to_child = child.stdin.take().ok_or("ACP stdin unavailable")?;
        let child_output = child.stdout.take().ok_or("ACP stdout unavailable")?;
        let mut output =
            FramedRead::new(child_output, LinesCodec::new_with_max_length(FRAME_LIMIT));
        let mut input = FramedRead::new(
            tokio::io::stdin(),
            LinesCodec::new_with_max_length(FRAME_LIMIT),
        );
        let mut stdout = tokio::io::stdout();
        let mut prompts = BTreeMap::<String, String>::new();
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .map_err(|_| "cannot observe shutdown")?;
        loop {
            tokio::select! {
                _ = terminate.recv() => break,
                _ = tokio::signal::ctrl_c() => break,
                line = input.next() => match line {
                    Some(Ok(line)) => {
                        let mut value: Value = serde_json::from_str(&line).map_err(|_| "invalid ACP input")?;
                        if value["method"] == "session/prompt" {
                            if let (Some(id), Some(session)) = (value.get("id"), value["params"]["sessionId"].as_str()) {
                                prompts.insert(id.to_string(), session.to_string());
                            }
                            if let Some(prompt) = value["params"]["prompt"].as_array_mut() {
                                prompt.insert(0, json!({"type":"text","text":format!(
                                    "[Server runtime context]\nYou run on the Linux Buzz Next host. For every Buzz CLI call use '{host}' cli {scope} followed by the command. This binds your agent identity, not the owner's. Never read or print launch.json or credential files. Deliver requested replies using this CLI and the current Buzz reply destination; ACP final text is not a delivered message. Desktop may be closed while you work.\n"
                                )}));
                            }
                        }
                        if ["session/new", "session/load", "session/resume", "session/fork"].contains(&value["method"].as_str().unwrap_or("")) {
                            // Shared model configuration owns its MCP registrations.
                            if let Some(params) = value["params"].as_object_mut() { params.insert("mcpServers".into(), json!([])); }
                        }
                        to_child.write_all(format!("{value}\n").as_bytes()).await.map_err(|_| "ACP input closed")?;
                    },
                    None => break,
                    _ => return Err("ACP input frame limit exceeded".into()),
                },
                line = output.next() => match line {
                    Some(Ok(line)) => {
                        let value: Value = serde_json::from_str(&line).map_err(|_| "invalid ACP output")?;
                        if value.get("method").is_none() { if let Some(id) = value.get("id") { prompts.remove(&id.to_string()); } }
                        stdout.write_all(line.as_bytes()).await.map_err(|_| "harness output closed")?;
                        stdout.write_all(b"\n").await.map_err(|_| "harness output closed")?;
                        stdout.flush().await.map_err(|_| "harness output closed")?;
                    },
                    None => return Err("ACP adapter exited".into()),
                    _ => return Err("ACP output frame limit exceeded".into()),
                }
            }
        }
        // Cancel only prompts owned by this adapter, never shared App Server work.
        for session in prompts.values() {
            let cancel =
                json!({"jsonrpc":"2.0","method":"session/cancel","params":{"sessionId":session}});
            let _ = to_child.write_all(format!("{cancel}\n").as_bytes()).await;
        }
        drop(to_child);
        let _ = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
        Ok(())
    }
}

#[cfg(unix)]
#[tokio::main(flavor = "current_thread")]
async fn main() {
    if let Err(error) = linux::run().await {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
#[cfg(not(unix))]
fn main() {
    eprintln!("shared Codex transport requires Unix sockets");
    std::process::exit(1);
}
