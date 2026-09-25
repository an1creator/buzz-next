//! JSONL to WebSocket-over-Unix transport for an existing shared Codex App Server.
use futures_util::{SinkExt, StreamExt};
use std::{path::PathBuf, time::Duration};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio_tungstenite::tungstenite::{protocol::WebSocketConfig, Message};
use tokio_util::codec::{FramedRead, LinesCodec};

const MAX_MESSAGE: usize = 10_000_000;

/// Explicit operator binding; validates paths without starting or authenticating Codex.
pub fn binding(
    bundle: &std::path::Path,
    socket: &str,
) -> Result<std::collections::BTreeMap<String, String>, String> {
    use std::os::unix::fs::FileTypeExt;
    let socket = std::path::Path::new(socket);
    if !socket.is_absolute()
        || !socket
            .metadata()
            .is_ok_and(|meta| meta.file_type().is_socket())
    {
        return Err("Shared Codex requires an existing absolute Unix socket".into());
    }
    crate::config::executable(&bundle.join("buzz-codex-connection"))?;
    let node = crate::setup::find_executable("node")
        .ok_or("Node is required for the pinned Codex ACP adapter")?;
    let script =
        bundle.join("codex-adapter/node_modules/@agentclientprotocol/codex-acp/dist/index.js");
    if !script.is_file() {
        return Err("Pinned Codex ACP adapter is missing from the bundle".into());
    }
    Ok([
        (
            "BUZZ_CODEX_SOCKET".into(),
            socket.to_string_lossy().into_owned(),
        ),
        (
            "BUZZ_CODEX_NODE".into(),
            node.to_string_lossy().into_owned(),
        ),
        (
            "BUZZ_CODEX_ACP_SCRIPT".into(),
            script.to_string_lossy().into_owned(),
        ),
    ]
    .into())
}

fn absolute_env(name: &str) -> Result<PathBuf, String> {
    let path = std::env::var_os(name)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .ok_or_else(|| format!("{name} must be an absolute path"))?;
    Ok(path)
}

/// Run the pinned ACP adapter, or serve its requested App Server stdio transport.
pub async fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args == ["--version"] {
        println!("buzz-codex-connection {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let socket = absolute_env("BUZZ_CODEX_SOCKET")?;
    if args == ["app-server"] {
        return bridge(&socket, tokio::io::stdin(), tokio::io::stdout()).await;
    }
    if !args.is_empty() {
        return Err("Shared Codex supports ACP or app-server transport only".into());
    }
    let script = absolute_env("BUZZ_CODEX_ACP_SCRIPT")?;
    if !script.is_file() {
        return Err("Pinned Codex ACP adapter is missing".into());
    }
    let node = absolute_env("BUZZ_CODEX_NODE")?;
    crate::config::executable(&node)?;
    let executable = std::env::current_exe().map_err(|_| "Cannot locate Codex connection")?;
    let status = tokio::process::Command::new(node)
        .arg(script)
        .env("CODEX_PATH", executable)
        .env_remove("DEFAULT_AUTH_REQUEST")
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .kill_on_drop(true)
        .status()
        .await
        .map_err(|_| "Cannot start pinned Codex ACP adapter")?;
    if status.success() {
        Ok(())
    } else {
        Err("Codex ACP adapter exited unsuccessfully".into())
    }
}

/// Bound memory and apply backpressure in both directions. Socket loss is an
/// error: reconnecting blindly could duplicate an accepted turn.
pub async fn bridge<R, W>(socket: &std::path::Path, input: R, mut output: W) -> Result<(), String>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let connect = async {
        let stream = tokio::net::UnixStream::connect(socket)
            .await
            .map_err(|_| "Shared Codex socket is unavailable")?;
        tokio_tungstenite::client_async_with_config(
            "ws://localhost/",
            stream,
            Some(
                WebSocketConfig::default()
                    .max_message_size(Some(MAX_MESSAGE))
                    .max_frame_size(Some(MAX_MESSAGE)),
            ),
        )
        .await
        .map_err(|_| "Shared Codex handshake failed")
    };
    let (mut peer, _) = tokio::time::timeout(Duration::from_secs(10), connect)
        .await
        .map_err(|_| "Shared Codex connection timed out")??;
    let mut lines = FramedRead::new(input, LinesCodec::new_with_max_length(MAX_MESSAGE));
    loop {
        tokio::select! {
            line = lines.next() => {
                let Some(line) = line else {
                    let _ = tokio::time::timeout(Duration::from_secs(2), peer.close(None)).await;
                    return Ok(());
                };
                let line = line.map_err(|_| "Invalid or oversized Codex request")?;
                let request: serde_json::Value = serde_json::from_str(&line)
                    .map_err(|_| "Invalid Codex request JSON")?;
                if matches!(request.get("method").and_then(|v| v.as_str()),
                    Some("account/login/start" | "account/logout" | "account/login/cancel")) {
                    let reply = serde_json::json!({"id":request.get("id"),"error":{
                        "code":-32601,"message":"Shared Codex authentication is managed by the server operator"
                    }});
                    output.write_all(reply.to_string().as_bytes()).await.map_err(|_| "Codex output closed")?;
                    output.write_all(b"\n").await.map_err(|_| "Codex output closed")?;
                    output.flush().await.map_err(|_| "Codex output closed")?;
                    continue;
                }
                peer.send(Message::Text(line.into())).await.map_err(|_| "Shared Codex disconnected")?;
            }
            frame = peer.next() => {
                match frame {
                    Some(Ok(Message::Text(text))) => {
                        let value: serde_json::Value = serde_json::from_str(&text)
                            .map_err(|_| "Invalid shared Codex response")?;
                        output.write_all(value.to_string().as_bytes()).await.map_err(|_| "Codex output closed")?;
                        output.write_all(b"\n").await.map_err(|_| "Codex output closed")?;
                        output.flush().await.map_err(|_| "Codex output closed")?;
                    }
                    Some(Ok(Message::Ping(_))) => {
                        peer.flush().await.map_err(|_| "Shared Codex disconnected")?;
                    }
                    Some(Ok(Message::Pong(_))) => {}
                    _ => return Err("Shared Codex disconnected; current request is unconfirmed".into()),
                }
            }
        }
    }
}
