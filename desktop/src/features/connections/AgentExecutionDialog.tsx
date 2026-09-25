import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import type { ManagedAgent, RespondToMode } from "@/shared/api/types";
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
import { Input } from "@/shared/ui/input";
import { Textarea } from "@/shared/ui/textarea";
import { EnvVarsEditor } from "@/features/agents/ui/EnvVarsEditor";
import { OwnerOnlyAccessField } from "@/features/agents/ui/OwnerOnlyAccessField";
import { useAgentAccessOwnerOnlyQuery } from "@/features/agents/useAgentAccessOwnerOnly";
import { showAgentProfileSyncWarning } from "@/features/agents/ui/agentProfileSyncWarning";
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
  const [name, setName] = useState(agent.name);
  const [envVars, setEnvVars] = useState(agent.envVars);
  const [prompt, setPrompt] = useState(agent.systemPrompt ?? "");
  const [access, setAccess] = useState<RespondToMode>(agent.respondTo);
  const [allowlist, setAllowlist] = useState(agent.respondToAllowlist);
  const accessPolicy = useAgentAccessOwnerOnlyQuery();
  const [pending, setPending] = useState(false);
  const [editing, setEditing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const queryClient = useQueryClient();
  async function save() {
    setPending(true);
    setError(null);
    try {
      const result = await invokeTauri<{
        agent: RawManagedAgent;
        profile_sync_error?: string | null;
      }>("update_managed_agent", {
        input: {
          pubkey: agent.pubkey,
          execution: value ?? undefined,
          expectedUpdatedAt: agent.updatedAt,
          name: name.trim() === agent.name ? undefined : name.trim(),
          envVars,
          systemPrompt:
            agent.personaId || prompt === (agent.systemPrompt ?? "")
              ? undefined
              : prompt.trim() || null,
          respondTo: access,
          respondToAllowlist: allowlist,
        },
      });
      const updated = fromRawManagedAgent(result.agent);
      if (result.profile_sync_error)
        showAgentProfileSyncWarning(updated.name, result.profile_sync_error);
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
      data-testid="edit-agent-dialog"
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
            data-testid="edit-agent-dialog-submit"
            disabled={
              pending ||
              editing ||
              !name.trim() ||
              accessPolicy.isPending ||
              accessPolicy.isError
            }
            onClick={() => void save()}
          >
            {pending ? "Saving…" : "Save changes"}
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
        <label className="block space-y-1 text-sm" htmlFor="edit-agent-name">
          Agent name
          <Input
            id="edit-agent-name"
            value={name}
            onChange={(event) => setName(event.target.value)}
            disabled={pending}
          />
        </label>
        <fieldset disabled={pending}>
          <ExecutionFields
            value={value}
            onChange={setValue}
            onEditingChange={setEditing}
          />
          <OwnerOnlyAccessField
            accessLocked={accessPolicy.data ?? true}
            allowlist={allowlist}
            disabled={pending || accessPolicy.isPending || accessPolicy.isError}
            mode={access}
            onAllowlistChange={setAllowlist}
            onModeChange={setAccess}
          />
          <details className="space-y-3">
            <summary className="cursor-pointer text-sm">Advanced</summary>
            {!agent.personaId && (
              <label
                className="block space-y-1 text-sm"
                htmlFor="edit-agent-instructions"
              >
                Instructions
                <Textarea
                  id="edit-agent-instructions"
                  value={prompt}
                  onChange={(event) => setPrompt(event.target.value)}
                />
              </label>
            )}
            <EnvVarsEditor
              value={envVars}
              onChange={setEnvVars}
              disabled={pending}
            />
          </details>
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
