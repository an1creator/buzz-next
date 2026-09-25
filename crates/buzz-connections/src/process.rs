//! Bounded subprocess I/O with process-tree containment on both platforms.
use process_wrap::tokio::*;
use std::{process::Stdio, time::Duration};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};

/// Captured output is limited even when a remote command is faulty.
pub struct Output {
    pub success: bool,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// Run one subprocess. Dropping the future cancels and kills its contained tree.
pub async fn run(
    mut command: tokio::process::Command,
    input: &[u8],
    timeout: Duration,
) -> Result<Output, String> {
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut command = CommandWrap::from(command);
    #[cfg(unix)]
    command.wrap(ProcessSession);
    #[cfg(windows)]
    {
        command
            .wrap(CreationFlags(
                windows::Win32::System::Threading::CREATE_NO_WINDOW,
            ))
            .wrap(JobObject);
    }
    command.wrap(KillOnDrop);
    let mut child = command
        .spawn()
        .map_err(|e| format!("Cannot start process: {e}"))?;
    let mut stdin = child.stdin().take().ok_or("Missing subprocess stdin")?;
    let stdout = child.stdout().take().ok_or("Missing subprocess stdout")?;
    let stderr = child.stderr().take().ok_or("Missing subprocess stderr")?;
    let operation = async {
        let send = async {
            stdin.write_all(input).await.map_err(|e| e.to_string())?;
            drop(stdin);
            Ok::<_, String>(())
        };
        let ((), stdout, stderr, status) = tokio::try_join!(
            send,
            bounded(stdout, 1_048_576),
            bounded(stderr, 65_536),
            async { child.wait().await.map_err(|e| e.to_string()) }
        )?;
        Ok(Output {
            success: status.success(),
            stdout,
            stderr,
        })
    };
    match tokio::time::timeout(timeout, operation).await {
        Ok(Ok(output)) => Ok(output),
        result => {
            let cause = match result {
                Ok(Err(error)) => error,
                _ => "Operation timed out".into(),
            };
            std::pin::Pin::from(child.kill())
                .await
                .map_err(|error| format!("{cause}; process containment failed: {error}"))?;
            Err(cause)
        }
    }
}

async fn bounded(reader: impl AsyncRead + Unpin, limit: usize) -> Result<Vec<u8>, String> {
    let mut data = Vec::new();
    reader
        .take((limit + 1) as u64)
        .read_to_end(&mut data)
        .await
        .map_err(|e| e.to_string())?;
    if data.len() > limit {
        return Err("Subprocess output exceeded its limit".into());
    }
    Ok(data)
}
