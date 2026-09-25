//! One bounded SSH request. A successful deploy ends this transport entirely.
use crate::{
    model::{CheckOutcome, SshEndpoint},
    process,
    prompt::{Answers, PromptBridge},
    ssh,
};
use serde_json::Value;
use std::{path::Path, time::Duration};

/// Transport failure is structured; raw SSH output is never used as UI copy.
#[derive(Debug)]
pub struct Failure {
    pub outcome: CheckOutcome,
    pub trust_prompt: Option<String>,
}

/// Send a request to the fixed installed host entry point, not a user-built shell command.
pub async fn call(
    endpoint: &SshEndpoint,
    helper: &Path,
    answers: Answers,
    request: &Value,
) -> Result<Value, Failure> {
    let failed = |message: &str| Failure {
        outcome: CheckOutcome::Failed {
            message: message.into(),
        },
        trust_prompt: None,
    };
    if !helper.is_file() {
        return Err(failed("SSH helper is missing. Reinstall Buzz Next."));
    }
    let bridge = PromptBridge::start(answers)
        .await
        .map_err(|_| failed("Cannot open the credential prompt"))?;
    let mut command = ssh::command(endpoint, helper);
    command.arg("exec \"$HOME/.local/bin/buzz-connections-host\" connections");
    bridge.configure(&mut command);
    let payload = zeroize::Zeroizing::new(
        serde_json::to_vec(request).map_err(|_| failed("Cannot encode server request"))?,
    );
    if payload.len() > 1_048_576 {
        return Err(failed("Server request exceeds its limit"));
    }
    let output = process::run(command, &payload, Duration::from_secs(45))
        .await
        .map_err(|_| {
            failed("SSH operation failed or timed out. Check the connection and retry.")
        })?;
    if let Some(trust) = bridge.pending_trust() {
        let fingerprint = trust
            .message
            .split_whitespace()
            .find(|part| part.starts_with("SHA256:"))
            .unwrap_or("Unknown fingerprint")
            .trim_end_matches('.')
            .to_string();
        return Err(Failure {
            outcome: CheckOutcome::HostTrustRequired { fingerprint },
            trust_prompt: Some(trust.message),
        });
    }
    if !output.success {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let outcome = if stderr.contains("REMOTE HOST IDENTIFICATION HAS CHANGED") {
            CheckOutcome::HostKeyChanged
        } else if stderr.contains("Permission denied") {
            CheckOutcome::AuthenticationRequired
        } else if stderr.contains("buzz-connections-host")
            && (stderr.contains("not found") || stderr.contains("No such file"))
        {
            CheckOutcome::SetupRequired
        } else {
            CheckOutcome::Unreachable
        };
        return Err(Failure {
            outcome,
            trust_prompt: None,
        });
    }
    let response: Value = serde_json::from_slice(&output.stdout)
        .map_err(|_| failed("Server returned an invalid response. Check its Buzz installation."))?;
    if response.get("ok") != Some(&Value::Bool(true)) {
        if response.get("error").and_then(Value::as_str) == Some("Server setup is required") {
            return Err(Failure {
                outcome: CheckOutcome::SetupRequired,
                trust_prompt: None,
            });
        }
        return Err(failed(
            response
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("Server operation failed"),
        ));
    }
    response
        .get("result")
        .cloned()
        .ok_or_else(|| failed("Server response has no result"))
}
