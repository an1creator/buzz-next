//! Real Windows OpenSSH client + GUI-subsystem askpass against a disposable SSH server.
#![cfg(windows)]
use buzz_connections::{
    model::{Authentication, SshEndpoint},
    process,
    prompt::{Answers, PromptBridge},
    ssh,
};
use std::{
    io::BufRead,
    path::Path,
    process::{Command, Stdio},
    time::Duration,
};
struct Server(std::process::Child);
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[tokio::test]
#[ignore = "platform gate installs paramiko for an isolated protocol fixture"]
async fn windows_openssh_password_encrypted_key_and_config() {
    let dir = tempfile::tempdir().unwrap();
    let key = dir.path().join("key with spaces");
    assert!(Command::new("ssh-keygen")
        .args([
            "-q",
            "-t",
            "rsa",
            "-b",
            "2048",
            "-N",
            "fixture-passphrase",
            "-f"
        ])
        .arg(&key)
        .status()
        .unwrap()
        .success());
    let mut server = Server(
        Command::new("python")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/ssh_server_fixture.py"
            ))
            .arg(key.with_extension("pub"))
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    );
    let mut port_line = String::new();
    std::io::BufReader::new(server.0.stdout.take().unwrap())
        .read_line(&mut port_line)
        .unwrap();
    let port: u16 = port_line.trim().parse().expect("SSH fixture port");
    let known_hosts = dir.path().join("known_hosts");
    let build = |endpoint: &SshEndpoint, bridge: &PromptBridge| {
        let base = ssh::command(endpoint, Path::new(env!("CARGO_BIN_EXE_buzz-ssh-askpass")));
        let mut command = tokio::process::Command::new("ssh");
        command
            .arg("-o")
            .arg(format!("UserKnownHostsFile={}", known_hosts.display()))
            .args(["-o", "GlobalKnownHostsFile=none"])
            .args(base.as_std().get_args())
            .arg("fixture-command");
        for (key, value) in base.as_std().get_envs() {
            if let Some(value) = value {
                command.env(key, value);
            }
        }
        bridge.configure(&mut command);
        command
    };
    let password = SshEndpoint::Manual {
        host: "127.0.0.1".into(),
        port,
        username: "fixture".into(),
        authentication: Authentication::Password {},
    };
    let bridge = PromptBridge::start(Answers::default()).await.unwrap();
    assert!(
        !process::run(build(&password, &bridge), b"", Duration::from_secs(20))
            .await
            .unwrap()
            .success
    );
    let prompt = bridge.pending_trust().expect("first host trust prompt");
    let mut answers = Answers::default();
    answers.password.push_str("fixture-password");
    answers.trusted_prompts.insert(prompt.message);
    let bridge = PromptBridge::start(answers).await.unwrap();
    let output = process::run(build(&password, &bridge), b"", Duration::from_secs(20))
        .await
        .unwrap();
    assert!(
        output.success,
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"BUZZ_WINDOWS_SSH_OK");
    let private_key = SshEndpoint::Manual {
        host: "127.0.0.1".into(),
        port,
        username: "fixture".into(),
        authentication: Authentication::PrivateKey {
            path: key.display().to_string(),
        },
    };
    let mut answers = Answers::default();
    answers.passphrase.push_str("fixture-passphrase");
    let bridge = PromptBridge::start(answers).await.unwrap();
    let output = process::run(build(&private_key, &bridge), b"", Duration::from_secs(20))
        .await
        .unwrap();
    assert!(
        output.success,
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"BUZZ_WINDOWS_SSH_OK");
    let config = dir.path().join("custom config");
    std::fs::write(&config, format!("Host fixture-alias\n HostName 127.0.0.1\n Port {port}\n User fixture\n IdentityFile \"{}\"\n IdentitiesOnly yes\n", key.display().to_string().replace('\\', "/"))).unwrap();
    let from_config = SshEndpoint::Config {
        path: config.display().to_string(),
        alias: "fixture-alias".into(),
    };
    let mut answers = Answers::default();
    answers.passphrase.push_str("fixture-passphrase");
    let bridge = PromptBridge::start(answers).await.unwrap();
    let output = process::run(build(&from_config, &bridge), b"", Duration::from_secs(20))
        .await
        .unwrap();
    assert!(
        output.success,
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"BUZZ_WINDOWS_SSH_OK");
    // The CI runner starts the Windows OpenSSH Authentication Agent service.
    // Add/remove only this disposable identity, never clear the user's agent.
    let agent_key = dir.path().join("agent fixture key");
    std::fs::copy(&key, &agent_key).unwrap();
    assert!(Command::new("ssh-keygen")
        .args(["-p", "-P", "fixture-passphrase", "-N", "", "-f"])
        .arg(&agent_key)
        .output()
        .unwrap()
        .status
        .success());
    assert!(Command::new("ssh-add")
        .arg(&agent_key)
        .output()
        .unwrap()
        .status
        .success());
    struct AgentKey(std::path::PathBuf);
    impl Drop for AgentKey {
        fn drop(&mut self) {
            let _ = Command::new("ssh-add").arg("-d").arg(&self.0).output();
        }
    }
    let _agent_key = AgentKey(agent_key);
    let endpoint = SshEndpoint::Manual {
        host: "127.0.0.1".into(),
        port,
        username: "fixture".into(),
        authentication: Authentication::Agent {},
    };
    let bridge = PromptBridge::start(Answers::default()).await.unwrap();
    let output = process::run(build(&endpoint, &bridge), b"", Duration::from_secs(20))
        .await
        .unwrap();
    assert!(
        output.success,
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"BUZZ_WINDOWS_SSH_OK");
}
