//! Host-side protocol endpoint and supervisor entrypoint.
#[cfg(unix)]
#[tokio::main(flavor = "current_thread")]
async fn main() {
    use buzz_backend_ssh::{host, read_request, reply, wire};
    use serde_json::json;
    let operation = std::env::args().nth(1).unwrap_or_default();
    if operation == "cli" {
        let result = host::config().and_then(|cfg| {
            host::cli(
                &cfg,
                &std::env::args().nth(2).unwrap_or_default(),
                &std::env::args().skip(3).collect::<Vec<_>>(),
            )
        });
        match result {
            Ok(code) => std::process::exit(code),
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
    }
    if operation == "run" {
        let result = host::config()
            .and_then(|cfg| host::run(&cfg, &std::env::args().nth(2).unwrap_or_default()));
        if let Err(error) = result {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }
    reply(match operation.as_str() {
        "info" => Ok(json!({"ok":true,"host_protocol":1,"version":env!("CARGO_PKG_VERSION")})),
        "deploy" => {
            async {
                let raw = read_request().await?;
                let request: wire::Deploy =
                    serde_json::from_value(raw).map_err(|_| "invalid host launch request")?;
                let cfg = host::config()?;
                let scope = host::deploy(&cfg, request).await?;
                Ok(json!({"ok":true,"agent_id":scope}))
            }
            .await
        }
        _ => Err("expected info, deploy or run".into()),
    });
}

#[cfg(not(unix))]
fn main() {
    eprintln!("buzz-host requires Linux and systemd");
    std::process::exit(1);
}
