# SSH provider and autonomous host

Read when installing or changing the SSH substrate. Product contract:
[remote-agent vision](../../VISION_REMOTE_AGENTS.md) and
[provider protocol](../../docs/remote-agents.md).

`buzz-backend-ssh` is a native Desktop-discovered protocol-v1 provider. It invokes
the installed OpenSSH client, requires a known host and noninteractive auth,
negotiates host protocol before transferring a secret-bearing launch request,
and exits after deployment. No persistent Desktop connection owns the agent.
Provider configuration contains only `host` and an installed `profile` name.

`buzz-host` loads `$HOME/.config/buzz-next/host.json`. The operator supplies:

- `state`: an absolute private directory outside versioned binaries;
- `relays`: allowed `wss://` community URLs;
- `profiles`: named objects with `command_name`, absolute `harness`, `command`,
  and `cli` paths, default `workspace`, and host-authoritative `env`.

The Desktop launch block supplies resolved policy/user environment and owner.
The profile maps a logical command such as `codex-acp` to installed server files.
`BUZZ_REMOTE_WORKSPACE` overrides the profile workspace and must name an existing
absolute directory. Per-channel workspace configuration is not implemented here.

The scope is SHA-256 of canonical community URL and the public key derived from
the supplied agent nsec. Deployment and process-lifetime locks prevent simultaneous
instances in this host state root. A running unit keeps its immutable launch
snapshot when Start is repeated; edited settings apply after exit and a new Start.
Separate independent host state roots are not a distributed singleton mechanism.

`buzz-next-agent@.service` is a user systemd template. It must be installed by the
operator; deployment cannot install software or modify shared Codex. The user
manager requires linger to survive the final logout. `Restart=no` preserves
intentional shutdown and inactivity exits. Instances are not enabled for boot:
crash, shutdown and reboot require an explicit Start. Inactivity is four hours;
the supervisor's maximum lifetime is seven days. These values are part of this
binding's lifecycle, independent of the harness's in-flight turn limits.

Snapshots contain credentials and use mode 0600 inside 0700 directories.
`buzz-host cli <scope> ...` executes the installed Buzz CLI with the snapshot's
identity. It never falls back to the operator's default key. Snapshots remain for
explicit subsequent starts and must not be printed or committed.

`buzz-codex-agent` provides native ACP framing and JSONL ↔ Unix WebSocket transport.
It uses the pinned official package in [codex-acp](codex-acp/package.json) for
ACP/Codex translation, so Node remains a server dependency. Windows needs neither
Node nor the former JavaScript SSH launcher for this provider. Required profile
environment: `BUZZ_CODEX_SOCKET`, `BUZZ_CODEX_ACP_ENTRY`, `CODEX_HOME`, and a `PATH`
that resolves Node. `HOME` belongs to the server account. The host adds the scoped
CLI binding automatically. Shared MCP configuration owns model tools; Desktop MCP
registrations are omitted at session creation. Shutdown only cancels prompts
submitted through this adapter and never starts/stops the shared App Server.

Tests: `cargo test -p buzz-backend-ssh` exercises the shipped host binary with a
fake supervisor and real filesystem locks. `cargo clippy -p buzz-backend-ssh
--all-targets -- -D warnings` checks native targets. These checks do not prove a
complete Windows → SSH → relay → Codex task; that requires live acceptance.
Candidate build jobs live in [the runtime workflow](../../.github/workflows/buzz-next-runtime.yml).
