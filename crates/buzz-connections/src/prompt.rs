//! Short-lived, token-authenticated askpass IPC. Passwords never enter argv/env.
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
use zeroize::Zeroizing;

/// Inputs exist in memory for one explicit operation; no Debug/Serialize implementation.
#[derive(Default)]
pub struct Answers {
    pub password: Zeroizing<String>,
    pub passphrase: Zeroizing<String>,
    pub trusted_prompts: BTreeSet<String>,
}

/// A host trust question withheld from OpenSSH until the user approves it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrustPrompt {
    pub message: String,
}

/// Loopback endpoint protected by a random per-operation capability.
pub struct PromptBridge {
    address: String,
    token: String,
    pending: Arc<Mutex<Option<TrustPrompt>>>,
    task: tokio::task::JoinHandle<()>,
}

impl PromptBridge {
    /// Start a bounded helper listener. Secrets are destroyed when the operation ends.
    pub async fn start(answers: Answers) -> Result<Self, String> {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|e| e.to_string())?;
        let address = listener
            .local_addr()
            .map_err(|e| e.to_string())?
            .to_string();
        let token = format!("{}{}", uuid::Uuid::new_v4(), uuid::Uuid::new_v4());
        let expected_token = token.clone();
        let pending = Arc::new(Mutex::new(None));
        let task_pending = pending.clone();
        let task = tokio::spawn(async move {
            for _ in 0..8 {
                let Ok(Ok((mut stream, _))) =
                    tokio::time::timeout(Duration::from_secs(45), listener.accept()).await
                else {
                    break;
                };
                let mut request = Zeroizing::new(Vec::new());
                let read = tokio::time::timeout(
                    Duration::from_secs(2),
                    (&mut stream).take(8193).read_to_end(&mut request),
                )
                .await;
                if !matches!(read, Ok(Ok(_))) || request.len() > 8192 {
                    continue;
                }
                let Ok(request) = serde_json::from_slice::<PromptRequest>(&request) else {
                    continue;
                };
                if request.token != expected_token {
                    continue;
                }
                let question = request.prompt.to_lowercase();
                let response = if question.contains("are you sure you want to continue connecting")
                {
                    if answers.trusted_prompts.contains(&request.prompt) {
                        "yes"
                    } else {
                        if let Ok(mut slot) = task_pending.lock() {
                            *slot = Some(TrustPrompt {
                                message: request.prompt.clone(),
                            });
                        }
                        "no"
                    }
                } else if question.contains("passphrase") {
                    &answers.passphrase
                } else if question.contains("password") {
                    &answers.password
                } else {
                    ""
                };
                let _ = tokio::time::timeout(
                    Duration::from_secs(2),
                    stream.write_all(response.as_bytes()),
                )
                .await;
            }
        });
        Ok(Self {
            address,
            token,
            pending,
            task,
        })
    }

    /// Configure only an ephemeral helper endpoint and capability, never SSH secrets.
    pub fn configure(&self, command: &mut tokio::process::Command) {
        command
            .env("BUZZ_ASKPASS_ADDRESS", &self.address)
            .env("BUZZ_ASKPASS_TOKEN", &self.token);
    }

    /// Pending trust remains separate from generic authentication failure.
    pub fn pending_trust(&self) -> Option<TrustPrompt> {
        self.pending.lock().ok().and_then(|slot| slot.clone())
    }
}
impl Drop for PromptBridge {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Private helper wire input; do not log it (contains an IPC capability).
#[derive(Serialize, Deserialize)]
pub struct PromptRequest {
    pub token: String,
    pub prompt: String,
}
