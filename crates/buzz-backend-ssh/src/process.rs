//! Deadline and output bounds for one-shot SSH/systemd operations.
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;

/// Run a process with bounded output and a deadline; never return its stderr.
pub async fn run(
    mut command: Command,
    input: &[u8],
    deadline: Duration,
) -> Result<Vec<u8>, String> {
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let mut child = command
        .spawn()
        .map_err(|_| "cannot launch required executable")?;
    let mut stdin = child.stdin.take().ok_or("child stdin unavailable")?;
    let stdout = child.stdout.take().ok_or("child stdout unavailable")?;
    let operation = async {
        let write = async {
            stdin
                .write_all(input)
                .await
                .map_err(|_| "child rejected request")?;
            stdin
                .shutdown()
                .await
                .map_err(|_| "cannot close child input")?;
            drop(stdin);
            Ok::<_, String>(())
        };
        let read = async {
            let mut bytes = Vec::new();
            stdout
                .take((crate::MAX_BYTES + 1) as u64)
                .read_to_end(&mut bytes)
                .await
                .map_err(|_| "cannot read child response")?;
            if bytes.len() > crate::MAX_BYTES {
                return Err("child output exceeds 1 MiB".into());
            }
            Ok::<_, String>(bytes)
        };
        let (_, bytes) = tokio::try_join!(write, read)?;
        let status = child.wait().await.map_err(|_| "cannot wait for child")?;
        if !status.success() {
            return Err("remote or supervisor operation failed; inspect host logs".into());
        }
        Ok(bytes)
    };
    tokio::time::timeout(deadline, operation)
        .await
        .map_err(|_| "operation deadline exceeded".to_string())?
}
