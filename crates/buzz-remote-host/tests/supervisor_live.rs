//! Isolated user-unit integration. Never reads production profiles or agent credentials.
#![cfg(unix)]
use buzz_connections::{
    process,
    wire::{DeploymentScope, HOST_PROTOCOL},
};
use nostr::{Keys, ToBech32};
use std::{os::unix::fs::PermissionsExt, time::Duration};

struct Unit(String);
impl Drop for Unit {
    fn drop(&mut self) {
        let _ = std::process::Command::new("systemctl")
            .args(["--user", "stop", &self.0])
            .output();
    }
}
async fn request(config: &std::path::Path, value: serde_json::Value) -> serde_json::Value {
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_buzz-host"));
    command
        .arg("connections")
        .env("BUZZ_CONNECTIONS_HOST_CONFIG", config);
    let output = process::run(
        command,
        &serde_json::to_vec(&value).unwrap(),
        Duration::from_secs(40),
    )
    .await
    .unwrap();
    assert!(output.success);
    let response: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(response["ok"], true, "{}", response["error"]);
    response["result"].clone()
}
#[tokio::test]
#[ignore = "requires systemd user manager; owns only a unique disposable test unit"]
async fn detached_launch_converges_and_stopped_request_does_not_resurrect() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().join("folder with spaces — test");
    std::fs::create_dir(&cwd).unwrap();
    let acp = dir.path().join("fixture-acp");
    std::fs::write(
        &acp,
        "#!/bin/sh\nprintf '%s\\n' \"$PWD\" >> launch-count\nwhile :; do sleep 1; done\n",
    )
    .unwrap();
    std::fs::set_permissions(&acp, std::fs::Permissions::from_mode(0o700)).unwrap();
    let keys = Keys::generate();
    let scope = DeploymentScope {
        server_id: uuid::Uuid::new_v4().to_string(),
        owner_pubkey: Keys::generate().public_key().to_hex(),
        relay_url: "wss://fixture.invalid".into(),
        agent_pubkey: keys.public_key().to_hex(),
    };
    let unit = Unit(format!("buzz-connection-{}.service", scope.key()));
    let config_path = dir.path().join("config.json");
    std::fs::write(&config_path, serde_json::to_vec(&serde_json::json!({
        "server_id":scope.server_id,"state_directory":dir.path().join("state"),"default_directory":cwd,"relay_urls":[scope.relay_url],"acp_binary":acp,"cli_binary":"/bin/true",
        "harnesses":[{"executable":"/bin/true","catalog":{"id":"fixture","label":"Fixture","avatar_url":"","availability":"available","default_args":[],"install_hint":"","install_instructions_url":"","can_auto_install":false,"requires_external_cli":false,"node_required":false,"auth_status":{"status":"not_applicable"},"source":"custom"}}]
    })).unwrap()).unwrap();
    let deployment = serde_json::json!({"op":"deploy","protocol":HOST_PROTOCOL,"request_id":uuid::Uuid::new_v4().to_string(),"server_id":scope.server_id,"harness_id":"fixture","directory":{"mode":"automatic"},"agent":{"relay_url":scope.relay_url,"private_key_nsec":keys.secret_key().to_bech32().unwrap(),"pubkey":scope.agent_pubkey,"respond_to":"owner-only","launch":{"owner_pubkey":scope.owner_pubkey}}});
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..8 {
        let path = config_path.clone();
        let value = deployment.clone();
        tasks.spawn(async move { request(&path, value).await });
    }
    let mut generation = None;
    while let Some(result) = tasks.join_next().await {
        let receipt = result.unwrap();
        assert_eq!(receipt["directory"], cwd.to_str().unwrap());
        if let Some(first) = &generation {
            assert_eq!(&receipt["generation"], first);
        } else {
            generation = Some(receipt["generation"].clone());
        }
    }
    // Every launcher has exited; the supervised process still runs exactly once.
    tokio::time::sleep(Duration::from_millis(200)).await;
    let count = std::fs::read_to_string(cwd.join("launch-count")).unwrap();
    assert_eq!(count.lines().count(), 1);
    assert_eq!(count.trim(), cwd.to_str().unwrap());
    let status_request = serde_json::json!({"op":"status","protocol":HOST_PROTOCOL,"scope":scope});
    assert_eq!(
        request(&config_path, status_request.clone()).await["state"],
        "running"
    );
    assert!(std::process::Command::new("systemctl")
        .args(["--user", "stop", &unit.0])
        .status()
        .unwrap()
        .success());
    assert_eq!(
        request(&config_path, status_request).await["state"],
        "stopped"
    );
    assert_eq!(request(&config_path, deployment).await["state"], "stopped");
    assert_eq!(
        std::fs::read_to_string(cwd.join("launch-count"))
            .unwrap()
            .lines()
            .count(),
        1
    );
}
