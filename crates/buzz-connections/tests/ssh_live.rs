//! Isolated SSH server: generated fixture keys only, no production config/credentials.
#![cfg(unix)]
use buzz_connections::{
    model::{Authentication, SshEndpoint},
    process,
    prompt::{Answers, PromptBridge},
    ssh,
};
use std::{
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
fn generate(path: &Path, passphrase: &str) {
    assert!(Command::new("ssh-keygen")
        .args(["-q", "-t", "ed25519", "-N", passphrase, "-f"])
        .arg(path)
        .status()
        .unwrap()
        .success());
}

#[tokio::test]
#[ignore = "requires local OpenSSH sshd; run explicitly in the platform gate"]
async fn encrypted_key_and_host_trust_through_real_openssh() {
    let dir = tempfile::tempdir().unwrap();
    let host_key = dir.path().join("host");
    let user_key = dir.path().join("identity space ü ; dollar$");
    generate(&host_key, "");
    generate(&user_key, "fixture-passphrase");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let config = dir.path().join("sshd_config");
    std::fs::write(&config, format!("Port {port}\nListenAddress 127.0.0.1\nHostKey {}\nPidFile {}\nAuthorizedKeysFile \"{}\"\nStrictModes no\nUsePAM no\nPasswordAuthentication no\nKbdInteractiveAuthentication no\nLogLevel ERROR\n", host_key.display(), dir.path().join("pid").display(), user_key.with_extension("pub").display())).unwrap();
    let _server = Server(
        Command::new("/usr/sbin/sshd")
            .args(["-D", "-e", "-f"])
            .arg(config)
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    );
    for attempt in 0..50 {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            break;
        }
        assert!(attempt < 49, "sshd did not start");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let endpoint = SshEndpoint::Manual {
        host: "127.0.0.1".into(),
        port,
        username: std::env::var("USER").unwrap(),
        authentication: Authentication::PrivateKey {
            path: user_key.display().to_string(),
        },
    };
    let known_hosts = dir.path().join("known_hosts");
    let run = |bridge: &PromptBridge| {
        let command = ssh::command(&endpoint, Path::new(env!("CARGO_BIN_EXE_buzz-ssh-askpass")));
        // Insert test-only host storage before destination, never alter user's known_hosts.
        let args: Vec<_> = command
            .as_std()
            .get_args()
            .map(|s| s.to_os_string())
            .collect();
        let env: Vec<_> = command
            .as_std()
            .get_envs()
            .filter_map(|(k, v)| v.map(|v| (k.to_os_string(), v.to_os_string())))
            .collect();
        let mut command = tokio::process::Command::new("ssh");
        command
            .arg("-o")
            .arg(format!("UserKnownHostsFile={}", known_hosts.display()))
            .args(["-o", "GlobalKnownHostsFile=/dev/null"])
            .args(args)
            .arg("printf BUZZ_SSH_OK")
            .envs(env);
        bridge.configure(&mut command);
        command
    };
    let mut answers = Answers::default();
    answers.passphrase.push_str("fixture-passphrase");
    let bridge = PromptBridge::start(answers).await.unwrap();
    let first = process::run(run(&bridge), b"", Duration::from_secs(10))
        .await
        .unwrap();
    assert!(!first.success);
    let trust = bridge
        .pending_trust()
        .expect("first connection must ask for trust");
    assert!(trust.message.contains("SHA256:"));
    let mut answers = Answers::default();
    answers.passphrase.push_str("fixture-passphrase");
    answers.trusted_prompts.insert(trust.message);
    let bridge = PromptBridge::start(answers).await.unwrap();
    let second = process::run(run(&bridge), b"", Duration::from_secs(10))
        .await
        .unwrap();
    assert!(
        second.success,
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    assert_eq!(second.stdout, b"BUZZ_SSH_OK");
    let mut wrong = Answers::default();
    wrong.passphrase.push_str("incorrect-passphrase");
    let wrong = PromptBridge::start(wrong).await.unwrap();
    assert!(
        !process::run(run(&wrong), b"", Duration::from_secs(10))
            .await
            .unwrap()
            .success
    );
    let alternate_host = dir.path().join("alternate-host");
    generate(&alternate_host, "");
    let wrong_host = std::fs::read_to_string(alternate_host.with_extension("pub")).unwrap();
    std::fs::write(&known_hosts, format!("[127.0.0.1]:{port} {wrong_host}")).unwrap();
    let changed = process::run(run(&bridge), b"", Duration::from_secs(10))
        .await
        .unwrap();
    assert!(!changed.success);
    assert!(
        String::from_utf8_lossy(&changed.stderr).contains("REMOTE HOST IDENTIFICATION HAS CHANGED")
    );
    let client_config = dir.path().join("client config ü");
    std::fs::write(&client_config, "Host broken-proxy\n  HostName fixture.invalid\n  ProxyCommand sh -c 'echo BUZZ_PROXY_UNAVAILABLE >&2; exit 17'\n").unwrap();
    let endpoint = SshEndpoint::Config {
        path: client_config.display().to_string(),
        alias: "broken-proxy".into(),
    };
    let mut command = ssh::command(&endpoint, Path::new(env!("CARGO_BIN_EXE_buzz-ssh-askpass")));
    command.arg("true");
    bridge.configure(&mut command);
    let proxy_failure = process::run(command, b"", Duration::from_secs(5))
        .await
        .unwrap();
    assert!(!proxy_failure.success);
    assert!(String::from_utf8_lossy(&proxy_failure.stderr).contains("BUZZ_PROXY_UNAVAILABLE"));
}
