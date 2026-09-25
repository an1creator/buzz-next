use buzz_connections::ssh_config;
#[test]
fn includes_enumerate_only_concrete_names_without_executing_match() {
    let home = tempfile::tempdir().unwrap();
    let ssh = home.path().join(".ssh");
    std::fs::create_dir(&ssh).unwrap();
    std::fs::write(ssh.join("config"), "Host server *.example !excluded\nInclude extra-*\nMatch exec \"touch must-not-run\"\n  User ignored\n").unwrap();
    std::fs::write(
        ssh.join("extra-one"),
        "Host=other\nHost \"space alias\"\nInclude config\n",
    )
    .unwrap();
    assert_eq!(
        ssh_config::aliases(&ssh.join("config"), home.path()).unwrap(),
        vec!["other", "server"]
    );
    assert!(!home.path().join("must-not-run").exists());
}
