# Connections execution host

Read when preparing an SSH server or inspecting the launch lifecycle.
[Remote Agents Vision](../../VISION_REMOTE_AGENTS.md) · [Host protocol](../buzz-connections/src/wire.rs)

`buzz-host` accepts bounded JSON on stdin. The Desktop calls the separate entry point
`$HOME/.local/bin/buzz-connections-host`; it does not replace another provider's host.
The supported server platform is Linux with a systemd user manager. A persistent
user manager (linger) is required for agents to survive logout. No service starts
on boot, after a crash, or after an intentional stop without a new Start.

## Install an exact candidate

Use the `Connections host candidate` artifact from the same source commit as the
Desktop candidate. Inspect `SOURCE_COMMIT` and `COMPATIBILITY.json`. Download it
from that exact successful GitHub Actions run, extract into a new directory, and
run `sha256sum -c SHA256SUMS`. Keep that directory immutable while agents use it.
Do not install an artifact from an untrusted run or bypass a failed checksum.

Make the bundle's five binaries executable and create a symlink from
`~/.local/bin/buzz-connections-host` to the versioned `buzz-host` binary.
Run that versioned binary explicitly:

```sh
/path/to/exact-candidate/buzz-host configure --relay wss://your-community --directory /absolute/existing/folder
```

Setup creates `~/.config/buzz-next/connections-host.json` and private supervisor
state in `~/.local/state/buzz-next/connections`. It refuses to overwrite an existing
configuration. It discovers already installed builtin harnesses from PATH and
uses the shared backend capability catalog. Installation and login for a vendor
harness are separate operator actions. The working folder must already exist.

For an update, validate the new bundle first, keep the server ID and state path,
then bind `acp_binary` and `cli_binary` in the operator configuration to that bundle
and update the entry-point symlink. Existing agent processes retain their frozen
paths and environment; retain every release still used by an active process.

## Data and lifecycle

The operator configuration selects absolute binaries, harness variants and allowed
relay URLs. Its `server_id` is stable across updates. Each deployment is scoped by
server ID, owner public key, canonical relay URL and agent public key. Local SSH
connection aliases never enter that scope.

`info`, `models`, `validate_directory` and `status` do not deploy agents. Authentication
probes run the installed vendor CLI; they do not read or return credential files.
A deploy transfers the agent identity over SSH stdin, negotiates host protocol
beforehand, takes a scope lock and writes a restricted immutable launch snapshot.
Snapshots contain secrets and must not be inspected, published or copied to Git.

A transient systemd user unit owns `buzz-acp` after SSH exits. Its policy is
`Restart=no`, a 7-day maximum lifetime and 4-hour ACP inactivity shutdown. Relay
presence and owner-authorized relay shutdown remain the conversational management
plane. `status` is an explicit recovery operation, never a background poller.

A repeated accepted request does not resurrect a stopped agent. Concurrent launches
return the current scope's receipt. The receipt records the applied harness, model
and folder; it proves handoff, not relay availability.

## Existing shared Codex

An explicit `--codex-socket /absolute/existing/socket` option on `configure` binds
the Codex entry to `buzz-codex-connection`. The bundle includes the locked
`@agentclientprotocol/codex-acp@1.10.0` adapter; Node must already be installed.
The native transport connects to the existing socket and never starts an App Server.
It translates bounded JSONL and WebSocket messages using the
[Codex App Server protocol](https://learn.chatgpt.com/docs/app-server#protocol).
Authentication remains with the server operator: login/logout requests are refused.
Socket loss is an error, not an automatic replay of a possibly accepted turn.

The operator profile pins `BUZZ_CODEX_SOCKET`, `BUZZ_CODEX_NODE` and
`BUZZ_CODEX_ACP_SCRIPT` to existing absolute paths. These bindings are hidden from
the public capability response. They do not define channel folders or replace the
agent's Execution directory. The socket is a trusted local service, not a security
boundary between agents sharing the server account.
