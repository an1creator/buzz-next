#![cfg(unix)]
//! Exercise the shipped host binary, actual file locks and snapshot boundaries.
use nostr::{Keys, ToBech32};
use serde_json::{json, Value};
use std::{
    fs,
    io::Write,
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Child, Command, Stdio},
};

fn write_executable(path: &Path, text: &str) {
    fs::write(path, text).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

struct Host {
    root: tempfile::TempDir,
    request: Value,
}
impl Host {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let p = root.path();
        fs::create_dir_all(p.join(".config/buzz-next")).unwrap();
        fs::create_dir(p.join("bin")).unwrap();
        fs::create_dir(p.join("workspace")).unwrap();
        write_executable(
            &p.join("bin/systemctl"),
            r#"#!/bin/sh
set -eu
case "$2" in
  show) if [ -f "$TEST_SUPERVISOR_STATE/active" ]; then printf 'active\n'; else printf 'inactive\n'; fi ;;
  start) printf 'start\n' >> "$TEST_SUPERVISOR_STATE/starts"; : > "$TEST_SUPERVISOR_STATE/active" ;;
  *) exit 2 ;;
esac
"#,
        );
        let keys = Keys::parse("0000000000000000000000000000000000000000000000000000000000000001")
            .unwrap();
        let nsec = keys.secret_key().to_bech32().unwrap();
        write_executable(
            &p.join("bin/test-cli"),
            &format!(
                "#!/bin/sh\n[ \"$BUZZ_PRIVATE_KEY\" = '{nsec}' ] && printf 'identity-bound\\n'\n"
            ),
        );
        fs::write(
            p.join(".config/buzz-next/host.json"),
            json!({
                "state":p.join("state"), "relays":["wss://example.com"],
                "profiles":{"shared-codex":{
                    "command_name":"codex-acp", "harness":"/bin/true", "command":"/bin/true",
                    "cli":p.join("bin/test-cli"),"workspace":p.join("workspace"),"env":{}
                }}
            })
            .to_string(),
        )
        .unwrap();
        let request = json!({"op":"deploy","provider_config":{"host":"test-host"},"agent":{
            "relay_url":"wss://example.com", "private_key_nsec":nsec,
            "launch":{"command":"codex-acp","owner_pubkey":keys.public_key().to_hex()}
        }});
        Self { root, request }
    }
    fn command(&self, op: &str) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_buzz-host"));
        command
            .arg(op)
            .env("HOME", self.root.path())
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", self.root.path().join("bin").display()),
            )
            .env("TEST_SUPERVISOR_STATE", self.root.path());
        command
    }
    fn start(&self, request: &Value) -> Child {
        let mut child = self
            .command("deploy")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(request.to_string().as_bytes())
            .unwrap();
        child
    }
    fn result(child: Child) -> Value {
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success());
        serde_json::from_slice(&output.stdout).unwrap()
    }
}

#[test]
fn concurrent_start_preserves_running_snapshot_and_single_instance() {
    let host = Host::new();
    let first = host.start(&host.request);
    let second = host.start(&host.request);
    let a = Host::result(first);
    let b = Host::result(second);
    assert_eq!(a["ok"], true, "{a}");
    assert_eq!(a, b);
    assert_eq!(
        fs::read_to_string(host.root.path().join("starts")).unwrap(),
        "start\n"
    );
    let scope = a["agent_id"].as_str().unwrap();
    let path = host
        .root
        .path()
        .join("state")
        .join(scope)
        .join("launch.json");
    let before = fs::read(&path).unwrap();
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let mut changed = host.request.clone();
    changed["agent"]["launch"]["env"] = json!({"MODEL":"changed"});
    assert_eq!(Host::result(host.start(&changed))["agent_id"], scope);
    assert_eq!(before, fs::read(&path).unwrap());
    let output = host.command("cli").arg(scope).output().unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"identity-bound\n");
    assert!(host.command("run").arg(scope).status().unwrap().success());
}

#[test]
fn invalid_identity_and_unallowed_community_create_no_instance() {
    let host = Host::new();
    let mut request = host.request.clone();
    request["agent"]["relay_url"] = json!("wss://not-allowed.example");
    assert_eq!(Host::result(host.start(&request))["ok"], false);
    request = host.request.clone();
    request["agent"]["pubkey"] = json!("a".repeat(64));
    assert_eq!(Host::result(host.start(&request))["ok"], false);
    assert!(!host.root.path().join("starts").exists());
}

#[test]
fn explicit_restart_after_exit_applies_new_snapshot() {
    let host = Host::new();
    let first = Host::result(host.start(&host.request));
    assert_eq!(first["ok"], true);
    fs::remove_file(host.root.path().join("active")).unwrap();
    let mut changed = host.request.clone();
    changed["agent"]["launch"]["env"] = json!({"MODEL":"changed"});
    let next = Host::result(host.start(&changed));
    assert_eq!(next, first);
    let snapshot: Value = serde_json::from_slice(
        &fs::read(
            host.root
                .path()
                .join("state")
                .join(first["agent_id"].as_str().unwrap())
                .join("launch.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(snapshot["env"]["MODEL"], "changed");
}
