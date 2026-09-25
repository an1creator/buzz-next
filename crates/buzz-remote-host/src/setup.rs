//! Explicit operator setup; ordinary discovery never installs or writes configuration.
use crate::config::{Config, Harness};
use buzz_connections::{catalog::*, harness_metadata, wire::canonical_relay};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

/// Resolve an installed executable using the SSH account's PATH.
pub fn find_executable(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path)
            .map(|directory| directory.join(name))
            .find(|path| crate::config::executable(path).is_ok())
            .and_then(|path| {
                if path.is_absolute() {
                    Some(path)
                } else {
                    std::env::current_dir().ok().map(|cwd| cwd.join(path))
                }
            })
    })
}

/// Prepare a configuration from a versioned bundle and already installed harnesses.
/// The caller writes it only after the explicit `configure` command.
#[cfg(unix)]
pub fn configuration(
    bundle: &Path,
    home: &Path,
    directory: &str,
    relay: &str,
    shared_codex_socket: Option<&str>,
) -> Result<Config, String> {
    let acp_binary = bundle.join("buzz-acp");
    let cli_binary = bundle.join("buzz");
    crate::config::executable(&acp_binary)?;
    crate::config::executable(&cli_binary)?;
    let shared_codex = shared_codex_socket
        .map(|socket| crate::codex_connection::binding(bundle, socket))
        .transpose()?;
    let mut harnesses = Vec::new();
    for metadata in harness_metadata::KNOWN_ACP_RUNTIMES {
        let Some(executable) = (if metadata.id == "codex" && shared_codex.is_some() {
            Some(bundle.join("buzz-codex-connection"))
        } else {
            metadata
                .commands
                .iter()
                .find_map(|name| find_executable(name))
        }) else {
            continue;
        };
        let cli = metadata.underlying_cli.and_then(find_executable);
        let args: Vec<String> = metadata
            .default_args
            .iter()
            .map(|arg| (*arg).into())
            .collect();
        let mut catalog = AcpRuntimeCatalogEntry {
            id: metadata.id.into(),
            label: metadata.label.into(),
            avatar_url: metadata.avatar_url.into(),
            availability: if metadata.underlying_cli.is_some() && cli.is_none() {
                AcpAvailabilityStatus::CliMissing
            } else {
                AcpAvailabilityStatus::Available
            },
            command: Some(executable.to_string_lossy().into_owned()),
            binary_path: Some(executable.to_string_lossy().into_owned()),
            default_args: args.clone(),
            mcp_command: metadata.mcp_command.map(str::to_owned),
            model_env_var: metadata.model_env_var.map(str::to_owned),
            provider_env_var: metadata.provider_env_var.map(str::to_owned),
            thinking_env_var: metadata.thinking_env_var.map(str::to_owned),
            effort_canonical_values: metadata.effort_normalization.map(|values| {
                values
                    .canonical
                    .iter()
                    .map(|value| (*value).into())
                    .collect()
            }),
            max_tokens_env_var: metadata.max_tokens_env_var.map(str::to_owned),
            context_limit_env_var: metadata.context_limit_env_var.map(str::to_owned),
            max_rounds_env_var: metadata.max_rounds_env_var.map(str::to_owned),
            install_hint: metadata.adapter_install_hint.into(),
            install_instructions_url: metadata.adapter_install_instructions_url.into(),
            can_auto_install: false,
            requires_external_cli: metadata.underlying_cli.is_some(),
            underlying_cli_path: cli.map(|path| path.to_string_lossy().into_owned()),
            node_required: false,
            auth_status: if metadata.auth_probe_args.is_some() {
                AuthStatus::Unknown
            } else {
                AuthStatus::NotApplicable
            },
            login_hint: metadata.login_hint.map(str::to_owned),
            source: HarnessSource::Builtin,
            definition_env: BTreeMap::new(),
            max_parallelism: None,
        };
        let mut environment: BTreeMap<String, String> = metadata
            .default_env
            .iter()
            .map(|(key, value)| ((*key).into(), (*value).into()))
            .collect();
        if metadata.id == "codex" {
            if let Some(binding) = &shared_codex {
                environment.extend(binding.clone());
                catalog.label = "Codex (shared server)".into();
                catalog.node_required = true;
                catalog.can_auto_install = false;
                catalog.login_hint =
                    Some("Authentication is managed by the server operator.".into());
            }
        }
        harnesses.push(Harness {
            catalog,
            executable,
            runtime_id: Some(metadata.id.into()),
            args,
            environment,
        });
    }
    let config = Config {
        server_id: uuid::Uuid::new_v4().to_string(),
        state_directory: home.join(".local/state/buzz-next/connections"),
        default_directory: directory.into(),
        relay_urls: vec![canonical_relay(relay)?],
        acp_binary,
        cli_binary,
        harnesses,
    };
    config.directory(&buzz_connections::model::WorkingDirectory::Automatic)?;
    Ok(config)
}

/// Explicit setup command. Existing identity and operator profiles are never overwritten.
#[cfg(unix)]
pub fn configure(args: &[String]) -> Result<(), String> {
    use std::{io::Write, os::unix::fs::DirBuilderExt, os::unix::fs::PermissionsExt};
    let home = PathBuf::from(std::env::var_os("HOME").ok_or("HOME is missing")?);
    let binary = std::env::current_exe().map_err(|_| "Cannot locate bundle")?;
    let bundle = binary.parent().ok_or("Cannot locate bundle")?;
    if !matches!(args.len(), 4 | 6)
        || args[0] != "--relay"
        || args[2] != "--directory"
        || (args.len() == 6 && args[4] != "--codex-socket")
    {
        return Err("Usage: buzz-host configure --relay wss://your-community --directory /absolute/existing/folder [--codex-socket /absolute/existing/socket]".into());
    }
    let config = configuration(
        bundle,
        &home,
        &args[3],
        &args[1],
        args.get(5).map(String::as_str),
    )?;
    let parent = home.join(".config/buzz-next");
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&parent)
        .map_err(|_| "Cannot create configuration directory")?;
    let path = parent.join("connections-host.json");
    use std::os::unix::fs::OpenOptionsExt;
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(parent.join("connections-setup.lock"))
        .map_err(|_| "Cannot lock server configuration")?;
    fs2::FileExt::try_lock_exclusive(&lock).map_err(|_| "Another setup is in progress")?;
    if path.exists() {
        return Err("Configuration already exists; existing profiles are preserved".into());
    }
    let mut file = atomic_write_file::AtomicWriteFile::open(&path)
        .map_err(|_| "Cannot prepare configuration")?;
    file.set_permissions(std::fs::Permissions::from_mode(0o600))
        .map_err(|_| "Cannot restrict configuration")?;
    let bytes = serde_json::to_vec_pretty(&config).map_err(|_| "Cannot encode configuration")?;
    file.write_all(&bytes)
        .map_err(|_| "Cannot save configuration")?;
    // State is separate from immutable bundles. No agent working folder is created.
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&config.state_directory)
        .map_err(|_| "Cannot prepare private host state")?;
    std::fs::set_permissions(
        &config.state_directory,
        std::fs::Permissions::from_mode(0o700),
    )
    .map_err(|_| "Cannot restrict host state")?;
    file.commit().map_err(|_| "Cannot commit configuration")?;
    Ok(())
}
