use buzz_connections::{
    model::SshEndpoint,
    process,
    prompt::{Answers, PromptBridge},
    ssh,
};
use std::time::Duration;
use tokio::process::Command;

#[tokio::test]
async fn askpass_returns_password_without_argv_or_environment_secret() {
    for _ in 0..12 {
        let mut answers = Answers::default();
        answers.password.push_str("test-password-only");
        let bridge = PromptBridge::start(answers).await.unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_buzz-ssh-askpass"));
        command.arg("user@host's password:");
        bridge.configure(&mut command);
        assert!(!format!("{command:?}").contains("test-password-only"));
        let result = process::run(command, b"", Duration::from_secs(5))
            .await
            .unwrap();
        assert!(
            result.success,
            "sanitized helper diagnostic: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(result.stdout, b"test-password-only\n");
    }
}

#[tokio::test]
async fn trust_requires_exact_explicit_approval_and_never_returns_password() {
    let prompt = "The authenticity of host cannot be established. SHA256:example. Are you sure you want to continue connecting (yes/no/[fingerprint])?";
    let mut answers = Answers::default();
    answers.password.push_str("secret");
    let bridge = PromptBridge::start(answers).await.unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_buzz-ssh-askpass"));
    command.arg(prompt);
    bridge.configure(&mut command);
    let result = process::run(command, b"", Duration::from_secs(5))
        .await
        .unwrap();
    assert_eq!(result.stdout, b"no\n");
    assert_eq!(bridge.pending_trust().unwrap().message, prompt);
    let mut answers = Answers::default();
    answers.trusted_prompts.insert(prompt.into());
    let bridge = PromptBridge::start(answers).await.unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_buzz-ssh-askpass"));
    command.arg(prompt);
    bridge.configure(&mut command);
    assert_eq!(
        process::run(command, b"", Duration::from_secs(5))
            .await
            .unwrap()
            .stdout,
        b"yes\n"
    );
}

#[tokio::test]
async fn encrypted_key_uses_passphrase_and_unknown_prompt_gets_nothing() {
    let mut answers = Answers::default();
    answers.passphrase.push_str("key-passphrase");
    let bridge = PromptBridge::start(answers).await.unwrap();
    for (question, expected) in [
        (
            "Enter passphrase for key '/file':",
            b"key-passphrase\n".as_slice(),
        ),
        ("Verification code:", b"\n".as_slice()),
    ] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_buzz-ssh-askpass"));
        command.arg(question);
        bridge.configure(&mut command);
        assert_eq!(
            process::run(command, b"", Duration::from_secs(5))
                .await
                .unwrap()
                .stdout,
            expected
        );
    }
}

#[test]
fn config_reference_and_remote_command_are_not_flattened() {
    let command = ssh::command(
        &SshEndpoint::Config {
            path: "C:/config with spaces".into(),
            alias: "development".into(),
        },
        std::path::Path::new("helper"),
    );
    let args: Vec<_> = command
        .as_std()
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        &args[args.len() - 4..],
        &["-F", "C:/config with spaces", "--", "development"]
    );
    assert!(!args
        .iter()
        .any(|arg| arg.contains("StrictHostKeyChecking=no")));
}

#[cfg(unix)]
#[tokio::test]
async fn timeout_and_output_flood_are_bounded() {
    let mut command = Command::new("sh");
    command.args(["-c", "sleep 30 & wait"]);
    let began = std::time::Instant::now();
    assert!(process::run(command, b"", Duration::from_millis(100))
        .await
        .is_err());
    assert!(began.elapsed() < Duration::from_secs(3));
    let mut command = Command::new("sh");
    command.args(["-c", "yes x"]);
    assert!(process::run(command, b"", Duration::from_secs(3))
        .await
        .is_err());
}
