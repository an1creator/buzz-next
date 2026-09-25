import type { ExecutionConnection } from "@/shared/api/tauriConnections";
/** In-memory IPC fixture; production registry/SSH behavior is tested in Rust. */
export function createConnectionMock() {
  const saved = new Map<string, ExecutionConnection>();
  const handlers: Record<
    string,
    (payload: Record<string, unknown>) => unknown
  > = {
    list_execution_connections: () => [...saved.values()],
    save_execution_connection: (payload) => {
      const value = payload.connection as ExecutionConnection;
      const previous = saved.get(value.id);
      if ((previous?.revision ?? null) !== payload.expectedRevision)
        throw new Error("Connection changed. Reload and retry.");
      const result = { ...value, revision: (previous?.revision ?? 0) + 1 };
      saved.set(value.id, result);
      return result;
    },
    execution_connection_has_credentials: () => false,
    execution_connection_dependents: () => [],
    delete_execution_connection: (payload) => {
      saved.delete(String(payload.id));
      return null;
    },
    cancel_execution_connection_check: () => null,
    pick_connection_file: (payload) =>
      payload.directory ? "C:\\Projects\\Sample" : "C:\\Keys\\my key",
    list_ssh_config_hosts: () => ({
      path: "C:/Users/test/.ssh/config",
      aliases: ["development"],
    }),
    resolve_ssh_config_host: () => ({
      host: "server.test",
      username: "engineer",
      port: "22",
    }),
    validate_execution_directory: () => ({ path: "/srv/projects" }),
    test_execution_connection: (payload) => {
      const connection = payload.connection as ExecutionConnection;
      const input = payload.input as { approved_host_prompts: string[] };
      const prompt = "SHA256:fixture-only server fingerprint";
      const trustRequired =
        connection.target.type === "ssh" &&
        !input.approved_host_prompts.includes(prompt);
      return {
        check: {
          revision: connection.revision,
          checked_at: 1750000000,
          outcome: trustRequired
            ? {
                status: "host_trust_required",
                fingerprint: "SHA256:fixture-only",
              }
            : { status: "ready" },
          server_id: trustRequired ? null : "fixture-server",
          default_directory: trustRequired ? null : "/srv/projects",
        },
        ticket: "fixture",
        trust_prompt: trustRequired ? prompt : null,
        harnesses: trustRequired
          ? []
          : [
              {
                id: "server-codex",
                label: "Codex — Server",
                avatar_url: "",
                availability: "available",
                command: "codex-acp",
                default_args: [],
                install_hint: "",
                install_instructions_url: "",
                can_auto_install: false,
                requires_external_cli: true,
                node_required: false,
                auth_status: { status: "logged_in" },
                source: "custom",
              },
            ],
      };
    },
    get_connection_models: () => ({
      agentName: "Server Codex",
      agentVersion: "fixture",
      models: [{ id: "server-model", name: "Server model", description: null }],
      agentDefaultModel: "server-model",
      selectedModel: null,
      supportsSwitching: true,
    }),
  };
  return {
    handles: (command: string) => command in handlers,
    invoke: (command: string, payload: unknown) =>
      handlers[command](payload as Record<string, unknown>),
  };
}
