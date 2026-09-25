import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import type { ManagedAgent } from "@/shared/api/types";
import type { AgentExecution } from "@/shared/api/tauriConnections";
import {
  invokeTauri,
  fromRawManagedAgent,
  type RawManagedAgent,
} from "@/shared/api/tauri";
import { Button } from "@/shared/ui/button";
import { Dialog } from "@/shared/ui/dialog";
import { ChooserDialogContent } from "@/shared/ui/chooser-dialog-content";
import { ExecutionFields } from "./ExecutionFields";
export function AgentExecutionDialog({
  agent,
  open,
  onOpenChange,
  onUpdated,
  onEditLinkedPersona,
}: {
  agent: ManagedAgent;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onUpdated?: (agent: ManagedAgent) => void;
  onEditLinkedPersona?: () => void;
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      {open && (
        <ExecutionDialogBody
          key={`${agent.pubkey}:${agent.updatedAt}`}
          agent={agent}
          onOpenChange={onOpenChange}
          onUpdated={onUpdated}
          onEditLinkedPersona={onEditLinkedPersona}
        />
      )}
    </Dialog>
  );
}
function ExecutionDialogBody({
  agent,
  onOpenChange,
  onUpdated,
  onEditLinkedPersona,
}: Omit<Parameters<typeof AgentExecutionDialog>[0], "open">) {
  const [value, setValue] = useState<AgentExecution | null>(
    agent.execution ?? null,
  );
  const [pending, setPending] = useState(false);
  const [editing, setEditing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const queryClient = useQueryClient();
  async function save() {
    if (!value) return;
    setPending(true);
    setError(null);
    try {
      const raw = await invokeTauri<RawManagedAgent>("set_agent_execution", {
        pubkey: agent.pubkey,
        execution: value,
        expectedUpdatedAt: agent.updatedAt,
      });
      const updated = fromRawManagedAgent(raw);
      await queryClient.invalidateQueries({ queryKey: ["managed-agents"] });
      onUpdated?.(updated);
      onOpenChange(false);
    } catch (error) {
      setError(String(error));
    } finally {
      setPending(false);
    }
  }
  return (
    <ChooserDialogContent
      title={`Edit ${agent.name}`}
      className="max-w-2xl"
      footer={
        <div className="flex justify-end gap-2">
          <Button
            type="button"
            variant="outline"
            disabled={pending}
            onClick={() => onOpenChange(false)}
          >
            Cancel
          </Button>
          <Button
            type="button"
            disabled={pending || editing || !value}
            onClick={() => void save()}
          >
            {pending ? "Saving…" : "Save execution"}
          </Button>
        </div>
      }
    >
      <div className="space-y-5">
        <div>
          <p className="text-sm text-muted-foreground">
            Execution settings belong to this agent instance. Its identity and
            conversations stay the same.
          </p>
          {onEditLinkedPersona && (
            <Button type="button" variant="ghost" onClick={onEditLinkedPersona}>
              Edit identity and instructions
            </Button>
          )}
        </div>
        {!agent.execution && (
          <p className="text-sm text-muted-foreground">
            Choose a connection explicitly before the next launch. Buzz has not
            converted your existing settings automatically.
          </p>
        )}
        <fieldset disabled={pending}>
          <ExecutionFields
            value={value}
            onChange={setValue}
            onEditingChange={setEditing}
          />
        </fieldset>
        <p className="text-sm text-muted-foreground">
          Applies on next start. Saving does not restart an agent. Changing
          machines requires its current instance to be stopped.
        </p>
        {error && (
          <p role="alert" className="text-sm text-destructive">
            {error}
          </p>
        )}
      </div>
    </ChooserDialogContent>
  );
}
