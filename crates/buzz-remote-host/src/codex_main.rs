//! Explicit binding to an already running Codex socket; never starts an App Server.
#[tokio::main]
async fn main() {
    #[cfg(unix)]
    let result = buzz_remote_host::codex_connection::run().await;
    #[cfg(not(unix))]
    let result: Result<(), String> = Err("Shared Codex connections require a Unix server".into());
    if let Err(error) = &result {
        eprintln!("{error}");
    }
    // Tokio stdin may still own a blocking read after peer disconnect. Do not
    // keep the adapter alive waiting for that read to complete.
    std::process::exit(if result.is_ok() { 0 } else { 1 });
}
