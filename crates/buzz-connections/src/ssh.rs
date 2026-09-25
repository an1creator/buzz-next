//! OpenSSH argument construction: user data never becomes a shell command.
use crate::model::{Authentication, SshEndpoint};
use std::path::Path;
use tokio::process::Command;

/// Build a command using native SSH configuration and host-key verification.
pub fn command(endpoint: &SshEndpoint, askpass: &Path) -> Command {
    let mut command = Command::new("ssh");
    command
        .env("SSH_ASKPASS", askpass)
        .env("SSH_ASKPASS_REQUIRE", "force")
        .env("DISPLAY", "buzz:0")
        .env("LC_ALL", "C");
    command.args([
        "-T",
        "-o",
        "StrictHostKeyChecking=ask",
        "-o",
        "ConnectTimeout=15",
        "-o",
        "ConnectionAttempts=1",
        "-o",
        "NumberOfPasswordPrompts=1",
        "-o",
        "ControlMaster=no",
        "-o",
        "ControlPath=none",
        "-o",
        "PermitLocalCommand=no",
        "-o",
        "ClearAllForwardings=yes",
    ]);
    match endpoint {
        SshEndpoint::Config { path, alias } => {
            command.arg("-F").arg(path).arg("--").arg(alias);
        }
        SshEndpoint::Manual {
            host,
            port,
            username,
            authentication,
        } => {
            // Manual details do not silently inherit a matching user's Host block.
            command.args(["-F", "none", "-p", &port.to_string(), "-l", username]);
            match authentication {
                Authentication::Password {} => {
                    command.args([
                        "-o",
                        "PreferredAuthentications=password,keyboard-interactive",
                        "-o",
                        "PubkeyAuthentication=no",
                    ]);
                }
                Authentication::PrivateKey { path } => {
                    command.args(["-o", "IdentitiesOnly=yes", "-i", path]);
                }
                Authentication::Agent {} => {
                    command.args([
                        "-o",
                        "PreferredAuthentications=publickey",
                        "-o",
                        "PasswordAuthentication=no",
                        "-o",
                        "KbdInteractiveAuthentication=no",
                    ]);
                }
            }
            command.arg("--").arg(host);
        }
    }
    command
}

/// Resolve config only following a user action; OpenSSH may evaluate Match exec.
pub fn effective_config(path: &str, alias: &str) -> Command {
    let mut command = Command::new("ssh");
    command.args(["-G", "-F", path, "--", alias]);
    command
}
