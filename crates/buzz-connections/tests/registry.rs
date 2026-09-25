use buzz_connections::{model::*, registry::Registry, wire::DeploymentScope};

fn local() -> Connection {
    Connection {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Laptop".into(),
        revision: 0,
        target: Target::Local,
        defaults: AgentDefaults::default(),
        check: None,
    }
}
fn ready(revision: u64) -> ConnectionCheck {
    ConnectionCheck {
        revision,
        checked_at: 1,
        outcome: CheckOutcome::Ready,
        server_id: None,
        default_directory: Some("/home/user".into()),
    }
}
#[test]
fn empty_registry_does_not_create_local_connection() {
    assert!(Registry::default().connections.is_empty());
}
#[test]
fn save_never_trusts_client_supplied_readiness() {
    let mut store = Registry::default();
    let mut draft = local();
    draft.check = Some(ready(1));
    let saved = store.save(draft, None).unwrap();
    assert!(!saved.checked_ready());
    assert_eq!(saved.revision, 1);
}
#[test]
fn one_device_and_stable_identity_across_rename() {
    let mut store = Registry::default();
    let mut value = store.save(local(), None).unwrap();
    assert!(store.save(local(), None).is_err());
    store.record_check(&value.id, ready(1)).unwrap();
    value.name = "Work laptop".into();
    let saved = store.save(value.clone(), Some(1)).unwrap();
    assert_eq!(saved.id, value.id);
    assert!(saved.checked_ready());
    assert!(store.save(value, Some(1)).is_err());
}
#[test]
fn access_changes_invalidate_check_and_late_result_is_rejected() {
    let mut store = Registry::default();
    let mut draft = local();
    draft.target = Target::Ssh {
        endpoint: SshEndpoint::Manual {
            host: "host".into(),
            port: 22,
            username: "user".into(),
            authentication: Authentication::Password {},
        },
    };
    let mut saved = store.save(draft, None).unwrap();
    store.record_check(&saved.id, ready(1)).unwrap();
    saved.target = Target::Ssh {
        endpoint: SshEndpoint::Config {
            path: "/home/user/.ssh/config".into(),
            alias: "server".into(),
        },
    };
    let saved = store.save(saved, Some(1)).unwrap();
    assert!(!saved.checked_ready());
    assert!(store.record_check(&saved.id, ready(1)).is_err());
}
#[test]
fn remove_refuses_references_and_revision_conflicts() {
    let mut store = Registry::default();
    let saved = store.save(local(), None).unwrap();
    assert!(store.remove(&saved.id, 1, &["agent".into()]).is_err());
    assert!(store.remove(&saved.id, 2, &[]).is_err());
    assert_eq!(store.connections.len(), 1);
    store.remove(&saved.id, 1, &[]).unwrap();
    assert!(store.connections.is_empty());
}
#[test]
fn password_cannot_be_deserialized_into_saved_config() {
    let raw = serde_json::json!({"method":"password", "password":"must-not-persist"});
    assert!(serde_json::from_value::<Authentication>(raw).is_err());
}
#[test]
fn incomplete_harness_is_saveable_but_remote_path_must_be_absolute() {
    let mut conn = local();
    conn.target = Target::Ssh {
        endpoint: SshEndpoint::Config {
            path: "C:\\Users\\Alice\\.ssh\\config".into(),
            alias: "server".into(),
        },
    };
    let mut execution = Execution {
        connection_id: conn.id.clone(),
        harness_id: None,
        model: None,
        directory: WorkingDirectory::Automatic,
    };
    assert!(execution.validate(&conn).is_ok());
    execution.directory = WorkingDirectory::Explicit {
        path: "~/code".into(),
    };
    assert!(execution.validate(&conn).is_err());
    execution.directory = WorkingDirectory::Explicit {
        path: "/home/user/code with spaces".into(),
    };
    assert!(execution.validate(&conn).is_ok());
}
#[test]
fn ssh_options_and_wildcard_aliases_are_not_destinations() {
    for alias in ["-oProxyCommand=bad", "*", "!host", "host name"] {
        let mut conn = local();
        conn.target = Target::Ssh {
            endpoint: SshEndpoint::Config {
                path: "/config".into(),
                alias: alias.into(),
            },
        };
        assert!(conn.validate().is_err(), "{alias}");
    }
}
#[test]
fn scope_is_framed_and_independent_of_connection_name() {
    let scope = DeploymentScope {
        server_id: "ab".into(),
        owner_pubkey: "c".into(),
        relay_url: "wss://relay".into(),
        agent_pubkey: "agent".into(),
    };
    let mut other = scope.clone();
    other.server_id = "a".into();
    other.owner_pubkey = "bc".into();
    assert_ne!(scope.key(), other.key());
    assert_eq!(scope.key().len(), 64);
}
