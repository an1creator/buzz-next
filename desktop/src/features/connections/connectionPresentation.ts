import type {
  ConnectionOutcome,
  ExecutionConnection,
} from "@/shared/api/tauriConnections";
export function connectionStatus(outcome?: ConnectionOutcome): string {
  if (!outcome) return "Not checked";
  switch (outcome.status) {
    case "ready":
      return "Ready to launch";
    case "unreachable":
      return "Server unreachable. Check the address, port, or network.";
    case "authentication_required":
      return "Sign-in failed. Check your username and credentials.";
    case "host_trust_required":
      return "Confirm the server identity to continue.";
    case "host_key_changed":
      return "Server identity changed. Verify the change with the server administrator.";
    case "setup_required":
      return "SSH works. Set up Buzz on this server.";
    case "update_required":
      return "Update the server component to continue.";
    case "harness_setup_required":
      return "Connected. Set up a harness before launching an agent.";
    case "failed":
      return outcome.message;
  }
}
export function emptyConnection(type: "local" | "ssh"): ExecutionConnection {
  return {
    id: crypto.randomUUID(),
    name: type === "local" ? "This device" : "",
    revision: 0,
    defaults: { harness_id: null, model: null },
    check: null,
    target:
      type === "local"
        ? { type }
        : {
            type,
            endpoint: {
              mode: "manual",
              host: "",
              port: 22,
              username: "",
              authentication: { method: "password" },
            },
          },
  };
}
export function savedCheck(connection: ExecutionConnection) {
  return connection.check?.revision === connection.revision
    ? connection.check
    : null;
}
