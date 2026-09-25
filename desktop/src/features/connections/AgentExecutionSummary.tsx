import { useState } from "react";
import { confirmExecutionStopped } from "@/shared/api/tauriManagedAgents";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useIdentityQuery } from "@/shared/api/hooks";
import { invokeTauri } from "@/shared/api/tauri";
import type { ManagedAgent } from "@/shared/api/types";
import type { AgentExecution } from "@/shared/api/tauriConnections";
import { Button } from "@/shared/ui/button";

type ExecutionStatus = {
  connection_name: string | null;
  desired: AgentExecution;
  applied: {
    harness_id: string | null;
    harness_label: string | null;
    model: string | null;
    directory: string | null;
  } | null;
  pending: boolean;
  launch_unconfirmed: boolean;
  remote: boolean;
};
export function AgentExecutionSummary({ agent }: { agent: ManagedAgent }) {
  const identity = useIdentityQuery();
  const client = useQueryClient();
  const [checking, setChecking] = useState(false);
  const [error, setError] = useState<string | null>(null);
  async function confirmStopped() {
    setChecking(true);
    setError(null);
    try {
      await confirmExecutionStopped(agent.pubkey);
      await client.invalidateQueries({ queryKey: ["managed-agents"] });
      await client.invalidateQueries({ queryKey: ["agent-execution-status"] });
    } catch (error) {
      setError(String(error));
    } finally {
      setChecking(false);
    }
  }
  const query = useQuery({
    queryKey: [
      "agent-execution-status",
      identity.data?.pubkey,
      agent.pubkey,
      agent.updatedAt,
      agent.status,
    ],
    queryFn: () =>
      invokeTauri<ExecutionStatus | null>("get_agent_execution_status", {
        pubkey: agent.pubkey,
      }),
    enabled: Boolean(agent.execution),
  });
  if (!agent.execution)
    return (
      <p className="text-sm text-muted-foreground">
        Choose a connection in Edit agent before its next launch.
      </p>
    );
  if (query.isPending)
    return (
      <p role="status" className="text-sm">
        Loading execution details…
      </p>
    );
  if (query.isError)
    return (
      <div>
        <p role="alert" className="text-sm text-destructive">
          Could not load execution details.
        </p>
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={() => void query.refetch()}
        >
          Retry
        </Button>
      </div>
    );
  const status = query.data;
  if (!status) return null;
  const applied = status.applied;
  return (
    <section aria-label="Execution" className="space-y-2 rounded-xl border p-4">
      <h3 className="text-sm font-semibold">
        {status.connection_name ?? "Connection unavailable"} ·{" "}
        {applied?.harness_label ??
          applied?.harness_id ??
          status.desired.harness_id ??
          "Choose a harness"}
      </h3>
      <p className="text-sm text-muted-foreground">
        {status.remote
          ? "Continues running when Buzz is closed."
          : "Runs on this device while Buzz and this computer stay available."}
      </p>
      {applied ? (
        <dl className="space-y-2 text-sm">
          <div>
            <dt className="text-muted-foreground">Launch directory</dt>
            <dd className="break-all">
              {applied.directory ?? "Unknown for this process"}
            </dd>
          </div>
          <div>
            <dt className="text-muted-foreground">Launch model</dt>
            <dd className="break-all">{applied.model ?? "Harness default"}</dd>
          </div>
        </dl>
      ) : (
        <p className="text-sm text-muted-foreground">
          No confirmed launch snapshot is available.
        </p>
      )}
      {status.launch_unconfirmed && (
        <p role="status" className="text-sm">
          Launch status unconfirmed. Start again to recover its status without
          creating a duplicate.
        </p>
      )}
      {status.pending && (
        <p role="status" className="text-sm">
          Saved execution changes apply on next start. This launch keeps its
          existing configuration.
        </p>
      )}
      {status.remote && (
        <div className="space-y-2">
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={checking}
            onClick={() => void confirmStopped()}
          >
            {checking ? "Checking server…" : "Confirm stopped state"}
          </Button>
          <p className="text-xs text-muted-foreground">
            After stopping this agent through Buzz, confirm its stopped state
            before assigning another connection.
          </p>
        </div>
      )}
      {error && (
        <p role="alert" className="text-sm text-destructive">
          {error}
        </p>
      )}
    </section>
  );
}
