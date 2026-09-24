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
    let starts: Vec<_> = (0..8).map(|_| host.start(&host.request)).collect();
    let mut results = starts.into_iter().map(Host::result);
    let a = results.next().unwrap();
    assert_eq!(a["ok"], true, "{a}");
    for result in results {
        assert_eq!(a, result);
    }
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

#[test]
fn existing_state_must_still_be_private_and_not_a_symlink() {
    let host = Host::new();
    let state = host.root.path().join("state");
    fs::create_dir(&state).unwrap();
    fs::set_permissions(&state, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(Host::result(host.start(&host.request))["ok"], false);
    fs::remove_dir(&state).unwrap();
    std::os::unix::fs::symlink(host.root.path(), &state).unwrap();
    assert_eq!(Host::result(host.start(&host.request))["ok"], false);
    assert!(!host.root.path().join("starts").exists());
}

#[test]
fn combined_profile_and_request_cannot_publish_an_unreadable_snapshot() {
    let mut host = Host::new();
    let config_path = host.root.path().join(".config/buzz-next/host.json");
    let mut config: Value = serde_json::from_slice(&fs::read(&config_path).unwrap()).unwrap();
    config["profiles"]["shared-codex"]["env"] = json!({"HOST_DATA":"x".repeat(600_000)});
    fs::write(config_path, config.to_string()).unwrap();
    host.request["agent"]["launch"]["env"] = json!({"CLIENT_DATA":"y".repeat(600_000)});
    let result = Host::result(host.start(&host.request));
    assert_eq!(result["ok"], false);
    assert_eq!(result["error"], "resolved launch snapshot exceeds 1 MiB");
    assert!(!host.root.path().join("starts").exists());
    for scope in fs::read_dir(host.root.path().join("state")).unwrap() {
        let scope = scope.unwrap().path();
        assert!(!scope.join("launch.tmp").exists());
        assert!(!scope.join("launch.json").exists());
    }
}

#[test]
fn workspace_snapshot_validates_directories_and_only_changes_after_exit() {
    let host = Host::new();
    let agent = host.root.path().join("agent-folder");
    let channel = host.root.path().join("channel-folder");
    fs::create_dir(&agent).unwrap();
    fs::create_dir(&channel).unwrap();
    let channel_id = "11111111-1111-4111-8111-111111111111";
    let mut request = host.request.clone();
    request["agent"]["launch"]["env"] = json!({"BUZZ_ACP_WORKSPACE": agent});
    request["agent"]["launch"]["policy_env"] =
        json!({"BUZZ_ACP_CHANNEL_WORKSPACES":json!({channel_id:channel}).to_string()});
    let first = Host::result(host.start(&request));
    assert_eq!(first["ok"], true);
    let file = host
        .root
        .path()
        .join("state")
        .join(first["agent_id"].as_str().unwrap())
        .join("launch.json");
    let before = fs::read(&file).unwrap();
    let snapshot: Value = serde_json::from_slice(&before).unwrap();
    assert_eq!(
        snapshot["workspace"],
        agent.canonicalize().unwrap().to_str().unwrap()
    );
    let channels: Value = serde_json::from_str(
        snapshot["env"]["BUZZ_ACP_CHANNEL_WORKSPACES"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        channels[channel_id],
        channel.canonicalize().unwrap().to_str().unwrap()
    );
    request["agent"]["launch"]["env"]["BUZZ_ACP_WORKSPACE"] = json!(channel);
    assert_eq!(Host::result(host.start(&request)), first);
    assert_eq!(fs::read(&file).unwrap(), before);
    fs::remove_file(host.root.path().join("active")).unwrap();
    assert_eq!(Host::result(host.start(&request)), first);
    let updated: Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
    assert_eq!(
        updated["workspace"],
        channel.canonicalize().unwrap().to_str().unwrap()
    );
    fs::remove_file(host.root.path().join("active")).unwrap();
    request["agent"]["launch"]["policy_env"]["BUZZ_ACP_CHANNEL_WORKSPACES"] =
        json!(json!({channel_id:host.root.path().join("missing")}).to_string());
    assert_eq!(Host::result(host.start(&request))["ok"], false);
    assert_eq!(
        fs::read_to_string(host.root.path().join("starts")).unwrap(),
        "start\nstart\n"
    );
}
