//! One-shot SSH provider and independently supervised host runtime.
#[cfg(unix)]
pub mod host;
pub mod process;
pub mod wire;

use tokio::io::AsyncReadExt;

/// Maximum request or response size, including secret-bearing launch data.
pub const MAX_BYTES: usize = 1_048_576;

/// Read a bounded request without reflecting malformed or secret input in errors.
pub async fn read_request() -> Result<serde_json::Value, String> {
    let mut bytes = Vec::new();
    tokio::time::timeout(
        std::time::Duration::from_secs(15),
        tokio::io::stdin()
            .take((MAX_BYTES + 1) as u64)
            .read_to_end(&mut bytes),
    )
    .await
    .map_err(|_| "request deadline exceeded")?
    .map_err(|_| "cannot read request")?;
    if bytes.len() > MAX_BYTES {
        return Err("request exceeds 1 MiB".into());
    }
    serde_json::from_slice(&bytes).map_err(|_| "invalid request JSON".into())
}

/// Emit the single provider response. Errors are generated locally, never stderr echoes.
pub fn reply(result: Result<serde_json::Value, String>) {
    let value = result.unwrap_or_else(|error| serde_json::json!({"ok":false,"error":error}));
    println!("{value}");
}
