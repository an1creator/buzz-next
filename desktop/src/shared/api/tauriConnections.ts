import {
  invokeTauri,
  fromRawAcpRuntimeCatalogEntry,
  type RawAcpRuntimeCatalogEntry,
} from "./tauri";
import type { AcpRuntimeCatalogEntry } from "./types";

export type SshAuthentication =
  | { method: "password" }
  | { method: "private_key"; path: string }
  | { method: "agent" };
export type SshEndpoint =
  | {
      mode: "manual";
      host: string;
      port: number;
      username: string;
      authentication: SshAuthentication;
    }
  | { mode: "config"; path: string; alias: string };
export type ConnectionTarget =
  | { type: "local" }
  | { type: "ssh"; endpoint: SshEndpoint };
export type ConnectionOutcome =
  | {
      status:
        | "ready"
        | "unreachable"
        | "authentication_required"
        | "host_key_changed"
        | "setup_required"
        | "harness_setup_required";
    }
  | { status: "host_trust_required"; fingerprint: string }
  | { status: "update_required"; required_protocol: number }
  | { status: "failed"; message: string };
export type ConnectionCheck = {
  revision: number;
  checked_at: number;
  outcome: ConnectionOutcome;
  server_id: string | null;
  default_directory: string | null;
};
export type ExecutionConnection = {
  id: string;
  name: string;
  revision: number;
  target: ConnectionTarget;
  defaults: { harness_id: string | null; model: string | null };
  check: ConnectionCheck | null;
};
export type WorkingDirectory =
  | { mode: "automatic" }
  | { mode: "explicit"; path: string };
export type AgentExecution = {
  connection_id: string;
  harness_id: string | null;
  model: string | null;
  directory: WorkingDirectory;
};
export type ConnectionCredentials = {
  password: string;
  passphrase: string;
  remember: boolean;
};
export type ConnectionProbeInput = {
  password: string;
  passphrase: string;
  approved_host_prompts: string[];
};
export type ConnectionProbe = {
  check: ConnectionCheck;
  harnesses: AcpRuntimeCatalogEntry[];
  trust_prompt: string | null;
  ticket: string;
};
export const listExecutionConnections = () =>
  invokeTauri<ExecutionConnection[]>("list_execution_connections");
export const saveExecutionConnection = (
  connection: ExecutionConnection,
  credentials?: ConnectionCredentials,
  checkTicket?: string,
) =>
  invokeTauri<ExecutionConnection>("save_execution_connection", {
    connection,
    expectedRevision: connection.revision || null,
    credentials: credentials ?? null,
    checkTicket: checkTicket ?? null,
  });
export const deleteExecutionConnection = (connection: ExecutionConnection) =>
  invokeTauri<void>("delete_execution_connection", {
    id: connection.id,
    expectedRevision: connection.revision,
  });
export const pickConnectionFile = (directory = false) =>
  invokeTauri<string | null>("pick_connection_file", { directory });
export async function testExecutionConnection(
  connection: ExecutionConnection,
  input: ConnectionProbeInput,
  operationId: string,
): Promise<ConnectionProbe> {
  const result = await invokeTauri<
    Omit<ConnectionProbe, "harnesses"> & {
      harnesses: RawAcpRuntimeCatalogEntry[];
    }
  >("test_execution_connection", { connection, input, operationId });
  return {
    ...result,
    harnesses: result.harnesses.map(fromRawAcpRuntimeCatalogEntry),
  };
}
export const cancelConnectionCheck = (operationId: string) =>
  invokeTauri<void>("cancel_execution_connection_check", { operationId });
export const listSshConfigHosts = (path?: string) =>
  invokeTauri<{ path: string; aliases: string[] }>("list_ssh_config_hosts", {
    path: path ?? null,
  });
export const resolveSshConfigHost = (path: string, alias: string) =>
  invokeTauri<{
    host: string | null;
    username: string | null;
    port: string | null;
  }>("resolve_ssh_config_host", { path, alias });

export const validateExecutionDirectory = (
  connection: ExecutionConnection,
  directory: WorkingDirectory,
  input: ConnectionProbeInput,
) =>
  invokeTauri<{ path: string }>("validate_execution_directory", {
    connectionId: connection.id,
    expectedRevision: connection.revision,
    directory,
    input,
  });
