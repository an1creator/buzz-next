use buzz_connections::{
    catalog::AcpRuntimeCatalogEntry, model::WorkingDirectory, wire::HOST_PROTOCOL,
};
use buzz_remote_host::{
    config::{Config, Harness},
    launch::{self, Agent, Deploy, Launch},
};
use nostr::{Keys, ToBech32};
use std::{collections::BTreeMap, path::Path};

fn config(directory: &Path) -> Config {
    let catalog: AcpRuntimeCatalogEntry = serde_json::from_value(serde_json::json!({
        "id":"fixture", "label":"Fixture", "avatar_url":"", "availability":"available",
        "default_args":[], "install_hint":"Set up on server", "install_instructions_url":"", "can_auto_install":false,
        "requires_external_cli":false,"node_required":false,"auth_status":{"status":"not_applicable"},"source":"custom"
    })).unwrap();
    Config {
        server_id: uuid::Uuid::new_v4().to_string(),
        state_directory: directory.join("state"),
        default_directory: directory.display().to_string(),
        relay_urls: vec!["wss://community.example".into()],
        acp_binary: "/bin/true".into(),
        cli_binary: "/bin/true".into(),
        harnesses: vec![Harness {
            catalog,
            executable: "/bin/true".into(),
            args: vec![],
            environment: BTreeMap::new(),
        }],
    }
}
fn request(config: &Config) -> Deploy {
    let keys = Keys::generate();
    Deploy {
        protocol: HOST_PROTOCOL,
        request_id: uuid::Uuid::new_v4().to_string(),
        server_id: config.server_id.clone(),
        harness_id: "fixture".into(),
        directory: WorkingDirectory::Automatic,
        agent: Agent {
            relay_url: "wss://community.example/".into(),
            private_key_nsec: keys.secret_key().to_bech32().unwrap(),
            pubkey: Some(keys.public_key().to_hex()),
            auth_tag: None,
            provider: None,
            respond_to: Some("owner-only".into()),
            respond_to_allowlist: vec![],
            launch: Launch {
                owner_pubkey: Keys::generate().public_key().to_hex(),
                env: BTreeMap::new(),
                policy_env: BTreeMap::new(),
            },
        },
    }
}
#[test]
fn discovery_is_read_only_and_does_not_expose_profile_environment() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = config(dir.path());
    config.harnesses[0]
        .environment
        .insert("API_TOKEN".into(), "fixture-secret".into());
    config.harnesses[0]
        .catalog
        .definition_env
        .insert("API_TOKEN".into(), "fixture-secret".into());
    let info = config.info().unwrap();
    assert!(!serde_json::to_string(&info)
        .unwrap()
        .contains("fixture-secret"));
    assert!(!config.state_directory.exists());
}
#[test]
fn explicit_missing_directory_does_not_fall_back_or_get_created() {
    let dir = tempfile::tempdir().unwrap();
    let config = config(dir.path());
    let missing = dir.path().join("missing");
    assert!(config
        .directory(&WorkingDirectory::Explicit {
            path: missing.display().to_string()
        })
        .is_err());
    assert!(!missing.exists());
    assert!(config
        .directory(&WorkingDirectory::Explicit {
            path: "relative".into()
        })
        .is_err());
}
#[cfg(unix)]
#[test]
fn snapshot_pins_identity_scope_and_paths_and_rejects_overrides() {
    let dir = tempfile::tempdir().unwrap();
    let config = config(dir.path());
    let mut request = request(&config);
    request
        .agent
        .launch
        .env
        .insert("BUZZ_PRIVATE_KEY".into(), "forged".into());
    request
        .agent
        .launch
        .env
        .insert("BUZZ_ACP_AGENT_COMMAND".into(), "/bin/sh".into());
    request
        .agent
        .launch
        .env
        .insert("BUZZ_ACP_NO_PRESENCE".into(), "1".into());
    let snapshot = launch::snapshot(&config, &request).unwrap();
    assert_eq!(
        snapshot.environment["BUZZ_PRIVATE_KEY"],
        request.agent.private_key_nsec
    );
    assert_eq!(snapshot.environment["BUZZ_ACP_AGENT_COMMAND"], "/bin/true");
    assert!(!snapshot.environment.contains_key("BUZZ_ACP_NO_PRESENCE"));
    assert_eq!(snapshot.receipt.scope.relay_url, "wss://community.example");
    assert!(!serde_json::to_string(&snapshot.receipt)
        .unwrap()
        .contains("nsec1"));
    assert!(!config.state_directory.exists());
}
#[test]
fn changed_host_or_protocol_is_refused_before_deployment() {
    let dir = tempfile::tempdir().unwrap();
    let config = config(dir.path());
    let mut request = request(&config);
    request.server_id = uuid::Uuid::new_v4().to_string();
    assert!(launch::snapshot(&config, &request).is_err());
    request.server_id = config.server_id.clone();
    request.protocol += 1;
    assert!(launch::snapshot(&config, &request).is_err());
    assert!(!config.state_directory.exists());
}
#[test]
fn community_url_canonicalization_rejects_credential_and_scope_ambiguity() {
    assert_eq!(
        launch::canonical_relay("wss://EXAMPLE.com/").unwrap(),
        "wss://example.com"
    );
    for url in [
        "https://example.com",
        "wss://user:password@example.com",
        "wss://example.com?tenant=x",
        "wss://example.com#other",
    ] {
        assert!(launch::canonical_relay(url).is_err());
    }
}
