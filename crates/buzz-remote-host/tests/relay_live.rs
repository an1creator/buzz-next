//! Opt-in live acceptance: disposable agent/channel, existing shared Codex only.
#![cfg(unix)]
use buzz_connections::{
    process,
    wire::{DeploymentScope, HOST_PROTOCOL},
};
use buzz_sdk::{build_add_member, build_archive, build_create_channel, build_message, Visibility};
use buzz_ws_client::{NostrWsConnection, RelayMessage};
use futures_util::FutureExt;
use nostr::{Keys, ToBech32};
use serde_json::{json, Value};
use std::{path::Path, time::Duration};

struct Unit(String);
impl Drop for Unit {
    fn drop(&mut self) {
        let _ = std::process::Command::new("systemctl")
            .args(["--user", "stop", &self.0])
            .output();
    }
}
async fn host(path: &Path, payload: Value) -> Value {
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_buzz-host"));
    command.env("BUZZ_CONNECTIONS_HOST_CONFIG", path);
    let output = process::run(
        command,
        &serde_json::to_vec(&payload).unwrap(),
        Duration::from_secs(40),
    )
    .await
    .unwrap();
    assert!(output.success);
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["ok"], true, "{}", result["error"]);
    result["result"].clone()
}
async fn publish(connection: &mut NostrWsConnection, keys: &Keys, builder: nostr::EventBuilder) {
    println!(
        "Publishing fixture kind {}",
        builder.clone().sign_with_keys(keys).unwrap().kind
    );
    let reply = connection
        .send_event(builder.sign_with_keys(keys).unwrap())
        .await
        .unwrap();
    assert!(
        reply.accepted,
        "Relay rejected fixture event: {}",
        reply.message
    );
    println!("Fixture event accepted: {}", reply.event_id);
}

#[tokio::test]
#[ignore = "requires explicit live owner key path, relay, bundle and shared Codex socket"]
async fn relay_task_after_launcher_exit_and_owner_stop() {
    // No production defaults, no credential output. Without explicit opt-in this does not run.
    if std::env::var("BUZZ_LIVE_ACCEPTANCE").as_deref() != Ok("1") {
        return;
    }
    let _ = rustls::crypto::ring::default_provider().install_default();
    let key_path = std::env::var("BUZZ_LIVE_OWNER_KEY_FILE").unwrap();
    let raw = zeroize::Zeroizing::new(std::fs::read_to_string(key_path).unwrap());
    let owner = Keys::parse(raw.trim()).expect("Invalid acceptance owner key");
    let relay = std::env::var("BUZZ_LIVE_RELAY").unwrap();
    let bundle = std::path::PathBuf::from(std::env::var("BUZZ_LIVE_BUNDLE").unwrap());
    let socket = std::env::var("BUZZ_LIVE_CODEX_SOCKET").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let cwd = temp.path().join("workspace with spaces — acceptance");
    std::fs::create_dir(&cwd).unwrap();
    let mut config = buzz_remote_host::setup::configuration(
        &bundle,
        temp.path(),
        cwd.to_str().unwrap(),
        &relay,
        Some(&socket),
    )
    .unwrap();
    config.state_directory = temp.path().join("state");
    let config_path = temp.path().join("host.json");
    std::fs::write(&config_path, serde_json::to_vec(&config).unwrap()).unwrap();
    let agent = Keys::generate();
    let agent_hex = agent.public_key().to_hex();
    let scope = DeploymentScope {
        server_id: config.server_id.clone(),
        owner_pubkey: owner.public_key().to_hex(),
        relay_url: relay.clone(),
        agent_pubkey: agent_hex.clone(),
    };
    let _unit = Unit(format!("buzz-connection-{}.service", scope.key()));
    let channel = uuid::Uuid::new_v4();
    println!("Connecting to acceptance relay");
    let mut connection = tokio::time::timeout(
        Duration::from_secs(30),
        NostrWsConnection::connect_authenticated(&relay, &owner, None),
    )
    .await
    .expect("Relay connection deadline exceeded")
    .unwrap();
    println!("Acceptance owner authenticated");
    publish(
        &mut connection,
        &owner,
        build_create_channel(
            channel,
            &format!("connections-test-{}", &channel.to_string()[..8]),
            Some(Visibility::Private),
            None,
            Some("Disposable Connections acceptance fixture"),
            Some(3600),
        )
        .unwrap(),
    )
    .await;
    publish(
        &mut connection,
        &owner,
        build_add_member(channel, &agent_hex, None).unwrap(),
    )
    .await;
    let result = std::panic::AssertUnwindSafe(async {
    let expires = nostr::Timestamp::now().as_secs() + 3600;
    let auth = buzz_sdk::nip_oa::compute_auth_tag(
        &owner,
        &agent.public_key(),
        &format!("created_at<{expires}"),
    )
    .unwrap();
    let request = json!({"op":"deploy", "protocol":HOST_PROTOCOL, "request_id":uuid::Uuid::new_v4().to_string(), "server_id":config.server_id, "harness_id":"codex", "directory":{"mode":"explicit","path":cwd}, "agent":{"relay_url":relay,"private_key_nsec":agent.secret_key().to_bech32().unwrap(),"pubkey":agent_hex,"auth_tag":auth,"respond_to":"owner-only","launch":{"owner_pubkey":scope.owner_pubkey,"env":{"RUST_LOG":"buzz_acp=debug","BUZZ_ACP_MODEL":"gpt-5.4","BUZZ_ACP_SYSTEM_PROMPT":"This is an isolated integration test. Answer only the assigned channel. Use the buzz-dev-mcp shell tool to run the Buzz CLI command supplied by the task. A final text answer is not a delivered message. Do not change files or inspect credentials.","BUZZ_ACP_MAX_TURN_DURATION":"180","BUZZ_ACP_IDLE_TIMEOUT":"60"}}}});
    println!("Launching disposable agent");
    let receipt = host(&config_path, request.clone()).await;
    println!("Launcher exited");
    assert_eq!(receipt["directory"], cwd.to_str().unwrap());
    assert_eq!(receipt["model"], "gpt-5.4");
    connection.send_raw(&json!(["REQ","acceptance",{"kinds":[9],"#h":[channel.to_string()],"since":nostr::Timestamp::now().as_secs()}])).await.unwrap();
    // The one-shot host has exited; a second task must still be accepted through relay.
    tokio::time::sleep(Duration::from_secs(8)).await;
    let mut received = Vec::new();
    for turn in 1..=2 {
        let marker = format!("CONNECTIONS_ACCEPTANCE_{}_{}", channel, turn);
        publish(
            &mut connection,
            &owner,
            build_message(
                channel,
                &format!("Use the buzz-dev-mcp shell tool to execute: buzz messages send --channel {channel} --content {marker} . Do not use another shell tool; the Buzz MCP shell holds this test agent identity. Confirm the command succeeds."),
                None,
                &[&agent_hex],
                false,
                &[],
                &[],
            )
            .unwrap(),
        )
        .await;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(180);
        loop {
            assert!(
                tokio::time::Instant::now() < deadline,
                "Agent response deadline exceeded for turn {turn}"
            );
            if let Ok(RelayMessage::Event { event, .. }) =
                connection.next_event(Duration::from_secs(5)).await
            {
                println!("Received channel event from {}", if event.pubkey == agent.public_key() { "agent" } else { "owner" });
                if event.pubkey == agent.public_key() && event.content.trim() == marker {
                    received.push(event.id.to_hex());
                    break;
                }
            }
        }
    }
    publish(
        &mut connection,
        &owner,
        build_message(channel, "!shutdown", None, &[&agent_hex], false, &[], &[]).unwrap(),
    )
    .await;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    loop {
        let status = host(
            &config_path,
            json!({"op":"status","protocol":HOST_PROTOCOL,"scope":scope}),
        )
        .await;
        if status["state"] == "stopped" {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "Owner Stop did not stop the unit"
        );
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    assert_eq!(host(&config_path, request).await["state"], "stopped");
    received
    }).catch_unwind().await;
    publish(&mut connection, &owner, build_archive(channel).unwrap()).await;
    let received = result.unwrap_or_else(|panic| std::panic::resume_unwind(panic));
    println!(
        "{}",
        json!({"launcher_exited":true,"response_event_ids":received,"owner_stop":true,"stopped_retry":true,"channel_archived":true})
    );
}
