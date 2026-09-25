//! Linux systemd user units own the child after the SSH handoff exits.
use crate::{config::Config, launch::Snapshot};
use buzz_connections::{
    process,
    wire::{DeploymentScope, LaunchReceipt, LaunchState},
};
use fs2::FileExt;
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use tokio::process::Command;

fn private_directory(path: &Path) -> Result<(), String> {
    if !path.exists() {
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(path)
            .map_err(|_| "Cannot create private host state".to_string())?;
    }
    let metadata = path
        .symlink_metadata()
        .map_err(|_| "Cannot inspect host state".to_string())?;
    if !metadata.is_dir() || metadata.permissions().mode() & 0o077 != 0 {
        return Err("Host state must be a private 0700 directory, not a symlink".into());
    }
    Ok(())
}

fn snapshot_path(config: &Config, scope: &DeploymentScope) -> PathBuf {
    config.state_directory.join(format!("{}.json", scope.key()))
}
fn unit(scope: &DeploymentScope) -> String {
    format!("buzz-connection-{}.service", scope.key())
}

async fn lock(config: &Config, scope: &DeploymentScope) -> Result<File, String> {
    private_directory(&config.state_directory)?;
    let path = config.state_directory.join(format!("{}.lock", scope.key()));
    if path
        .symlink_metadata()
        .is_ok_and(|meta| meta.file_type().is_symlink())
    {
        return Err("Deployment lock is a symlink".into());
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(path)
        .map_err(|_| "Cannot lock deployment".to_string())?;
    let started = Instant::now();
    loop {
        match file.try_lock_exclusive() {
            Ok(()) => return Ok(file),
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock
                    && started.elapsed() < Duration::from_secs(20) =>
            {
                tokio::time::sleep(Duration::from_millis(50)).await
            }
            Err(_) => {
                return Err(
                    "Another launch is still in progress. Check its status before retrying.".into(),
                )
            }
        }
    }
}

fn read_snapshot(path: &Path) -> Result<Option<Snapshot>, String> {
    let metadata = match path.symlink_metadata() {
        Ok(metadata) => metadata,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("Cannot inspect launch snapshot".into()),
    };
    if !metadata.is_file() || metadata.permissions().mode() & 0o077 != 0 {
        return Err("Launch snapshot must be a private file, not a symlink".into());
    }
    let file = match File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("Cannot read launch snapshot".into()),
    };
    let mut bytes = zeroize::Zeroizing::new(Vec::new());
    file.take(1_048_577)
        .read_to_end(&mut bytes)
        .map_err(|_| "Cannot read launch snapshot".to_string())?;
    if bytes.len() > 1_048_576 {
        return Err("Launch snapshot exceeds its limit".into());
    }
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| "Launch snapshot is invalid".into())
}

fn write_snapshot(path: &Path, snapshot: &Snapshot) -> Result<(), String> {
    let mut file = atomic_write_file::AtomicWriteFile::open(path)
        .map_err(|_| "Cannot prepare launch snapshot".to_string())?;
    file.set_permissions(std::fs::Permissions::from_mode(0o600))
        .map_err(|_| "Cannot restrict snapshot permissions".to_string())?;
    let bytes = zeroize::Zeroizing::new(
        serde_json::to_vec(snapshot).map_err(|_| "Cannot encode snapshot".to_string())?,
    );
    if bytes.len() > 1_048_576 {
        return Err("Launch snapshot exceeds its limit".into());
    }
    file.write_all(&bytes)
        .map_err(|_| "Cannot write snapshot".to_string())?;
    file.commit().map_err(|_| "Cannot commit snapshot".into())
}

async fn active(scope: &DeploymentScope) -> Result<bool, String> {
    let mut command = Command::new("systemctl");
    command.args([
        "--user",
        "show",
        "--property=LoadState,ActiveState",
        &unit(scope),
    ]);
    let output = process::run(command, b"", Duration::from_secs(10)).await?;
    let text =
        std::str::from_utf8(&output.stdout).map_err(|_| "Invalid service state".to_string())?;
    let properties: std::collections::BTreeMap<_, _> = text
        .lines()
        .filter_map(|line| line.split_once('='))
        .collect();
    if properties.get("LoadState") == Some(&"not-found") {
        return Ok(false);
    }
    if !output.success {
        return Err("Cannot query user service manager; launch status is unconfirmed".into());
    }
    match properties.get("ActiveState").copied() {
        Some("active" | "activating" | "reloading" | "deactivating") => Ok(true),
        Some("inactive" | "failed") => Ok(false),
        _ => Err("Unknown service state; launch status is unconfirmed".into()),
    }
}

/// Retry converges to the existing unit and returns its actual applied snapshot.
pub async fn deploy(config: &Config, snapshot: Snapshot) -> Result<LaunchReceipt, String> {
    let _guard = lock(config, &snapshot.receipt.scope).await?;
    let path = snapshot_path(config, &snapshot.receipt.scope);
    let previous = read_snapshot(&path)?;
    if active(&snapshot.receipt.scope).await? {
        let mut receipt = previous
            .ok_or("Service is active without a readable snapshot; inspect the server")?
            .receipt;
        receipt.state = LaunchState::Running;
        return Ok(receipt);
    }
    // Replaying an accepted request after owner shutdown must not resurrect it.
    if let Some(previous) = previous {
        if previous.receipt.request_id == snapshot.receipt.request_id
            && previous.receipt.state == LaunchState::Running
        {
            let mut receipt = previous.receipt;
            receipt.state = LaunchState::Stopped;
            return Ok(receipt);
        }
    }
    write_snapshot(&path, &snapshot)?;
    let executable =
        std::env::current_exe().map_err(|_| "Cannot resolve host binary".to_string())?;
    let mut command = Command::new("systemd-run");
    command
        .args([
            "--user",
            "--quiet",
            "--collect",
            "--unit",
            &unit(&snapshot.receipt.scope),
            "--property=Restart=no",
            "--property=RuntimeMaxSec=604800",
            "--property=TimeoutStopSec=60",
            "--property=UMask=0077",
        ])
        .arg(executable)
        .arg("run-snapshot")
        .arg(&path);
    let result = process::run(command, b"", Duration::from_secs(20)).await;
    match result {
        Ok(output) if output.success => {
            let mut snapshot = snapshot;
            snapshot.receipt.state = LaunchState::Running;
            write_snapshot(&path, &snapshot)?;
            Ok(snapshot.receipt)
        }
        _ => {
            let mut receipt = snapshot.receipt;
            receipt.state = LaunchState::Unconfirmed;
            Ok(receipt)
        }
    }
}

/// Explicit recovery query; never used as a continuous presence poller.
pub async fn status(
    config: &Config,
    scope: &DeploymentScope,
) -> Result<Option<LaunchReceipt>, String> {
    if scope.server_id != config.server_id {
        return Err("Server identity changed".into());
    }
    // Serialize recovery with deploy so an in-flight handoff cannot look stopped.
    // A fresh host has no state and a status query must remain read-only.
    if !config.state_directory.exists() {
        return if active(scope).await? {
            Err("Service is active without host state; inspect the server".into())
        } else {
            Ok(None)
        };
    }
    let _guard = lock(config, scope).await?;
    let Some(snapshot) = read_snapshot(&snapshot_path(config, scope))? else {
        return if active(scope).await? {
            Err("Service is active without a readable snapshot; inspect the server".into())
        } else {
            Ok(None)
        };
    };
    if snapshot.receipt.scope != *scope {
        return Err("Launch scope mismatch".into());
    }
    let mut receipt = snapshot.receipt;
    receipt.state = if active(scope).await? {
        LaunchState::Running
    } else {
        LaunchState::Stopped
    };
    Ok(Some(receipt))
}

/// Executed only by the supervisor, never as a long-lived child of SSH.
pub async fn run_snapshot(path: &Path) -> Result<(), String> {
    let mut snapshot = read_snapshot(path)?.ok_or("Launch snapshot is missing")?;
    snapshot.receipt.state = LaunchState::Running;
    write_snapshot(path, &snapshot)?;
    crate::config::executable(&snapshot.acp_binary)?;
    let mut command = Command::new(&snapshot.acp_binary);
    command
        .env_clear()
        .envs(&snapshot.environment)
        .current_dir(&snapshot.receipt.directory)
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    for key in ["HOME", "USER", "LOGNAME", "XDG_RUNTIME_DIR"] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    let status = command
        .status()
        .await
        .map_err(|_| "Cannot start agent from snapshot".to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err("Agent exited with an error; inspect its service log".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_reader_refuses_shared_permissions_and_symlinks_before_parsing() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snapshot.json");
        std::fs::write(&path, "not json").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(read_snapshot(&path).err().unwrap().contains("private file"));
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(
            read_snapshot(&path).err().unwrap(),
            "Launch snapshot is invalid"
        );
        let link = directory.path().join("link.json");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(read_snapshot(&link).err().unwrap().contains("private file"));
    }
}
