import { useState } from "react";
import { Button } from "@/shared/ui/button";
import { HarnessesSettingsPanel } from "@/features/settings/ui/HarnessesSettingsPanel";
import { PreventSleepSettingsCard } from "@/features/settings/ui/PreventSleepSettingsCard";
import { SettingsOptionGroup } from "@/features/settings/ui/SettingsOptionGroup";
import {
  saveExecutionConnection,
  deleteExecutionConnection,
  type ExecutionConnection,
  type ConnectionProbe,
} from "@/shared/api/tauriConnections";
import { invokeTauri } from "@/shared/api/tauri";
import { ConnectionModelField } from "./ConnectionModelField";
import { ChoiceField } from "./ConnectionFields";
import { ConnectionCheckResult } from "./ConnectionCheckResult";
import { connectionStatus, savedCheck } from "./connectionPresentation";
import { useConnectionProbe } from "./useConnectionProbe";

export function ConnectionDetail({
  connection,
  initialProbe,
  onEdit,
  onBack,
  onSaved,
  onDeleted,
}: {
  connection: ExecutionConnection;
  initialProbe?: ConnectionProbe;
  onEdit: () => void;
  onBack: () => void;
  onSaved: (value: ExecutionConnection) => void;
  onDeleted: () => void;
}) {
  const probe = useConnectionProbe();
  const result = probe.result ?? initialProbe;
  const check = result?.check ?? savedCheck(connection);
  const [defaults, setDefaults] = useState(connection.defaults);
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);
  const [dependents, setDependents] = useState<
    { pubkey: string; name: string }[] | null
  >(null);
  const harnesses = result?.harnesses;
  async function saveDefaults() {
    setPending(true);
    setError(null);
    try {
      onSaved(
        await saveExecutionConnection(
          { ...connection, defaults },
          undefined,
          result?.ticket,
        ),
      );
    } catch (error) {
      setError(String(error));
    } finally {
      setPending(false);
    }
  }
  async function inspectDelete() {
    setPending(true);
    setError(null);
    try {
      setDependents(
        await invokeTauri<{ pubkey: string; name: string }[]>(
          "execution_connection_dependents",
          {
            id: connection.id,
          },
        ),
      );
    } catch (error) {
      setError(String(error));
    } finally {
      setPending(false);
    }
  }
  async function remove() {
    setPending(true);
    setError(null);
    try {
      await deleteExecutionConnection(connection);
      onDeleted();
    } catch (error) {
      setError(String(error));
    } finally {
      setPending(false);
    }
  }
  return (
    <div className="space-y-6" data-testid="connection-detail">
      <Button type="button" variant="ghost" onClick={onBack}>
        Back to connections
      </Button>
      <div>
        <h3 className="font-semibold">{connection.name}</h3>
        <p className="text-sm text-muted-foreground">
          {connection.target.type === "local"
            ? "This device"
            : "SSH · Continues running when Buzz is closed"}
        </p>
      </div>
      {(error || probe.error) && (
        <p role="alert" className="text-sm text-destructive">
          {error || probe.error}
        </p>
      )}
      <SettingsOptionGroup title="Connection details">
        <div className="space-y-3 p-4 text-sm">
          <p>{connectionStatus(check?.outcome)}</p>
          {connection.target.type === "ssh" && (
            <p className="break-all text-muted-foreground">
              {connection.target.endpoint.mode === "manual"
                ? `${connection.target.endpoint.username}@${connection.target.endpoint.host}:${connection.target.endpoint.port}`
                : `${connection.target.endpoint.alias} · ${connection.target.endpoint.path}`}
            </p>
          )}
          {check && (
            <p className="text-muted-foreground">
              Last checked: {new Date(check.checked_at * 1000).toLocaleString()}
            </p>
          )}
          <div className="flex flex-wrap gap-2">
            <Button type="button" variant="outline" onClick={onEdit}>
              Edit connection
            </Button>
            <Button
              type="button"
              variant="outline"
              disabled={probe.pending}
              onClick={() =>
                void probe.check(connection, {
                  password: "",
                  passphrase: "",
                  approved_host_prompts: [],
                })
              }
            >
              Check again
            </Button>
          </div>
          {probe.pending && (
            <p role="status">Checking connection and harnesses…</p>
          )}
        </div>
      </SettingsOptionGroup>
      {connection.target.type === "local" ? (
        <>
          <HarnessesSettingsPanel />
          <PreventSleepSettingsCard />
        </>
      ) : (
        <SettingsOptionGroup title="Harnesses">
          <div className="space-y-3 p-4 text-sm">
            {result ? (
              <>
                <ConnectionCheckResult
                  result={result}
                  onTrust={(prompt) =>
                    void probe.check(connection, {
                      password: "",
                      passphrase: "",
                      approved_host_prompts: [prompt],
                    })
                  }
                />
                {harnesses?.length === 0 &&
                  ["ready", "harness_setup_required"].includes(
                    result.check.outcome.status,
                  ) && <p>No harnesses found on this server.</p>}
                {harnesses?.map((harness) => (
                  <details key={harness.id}>
                    <summary className="cursor-pointer">
                      {harness.label} — setup details
                    </summary>
                    <p className="mt-2 whitespace-pre-wrap text-muted-foreground">
                      {harness.loginHint ||
                        harness.installHint ||
                        "Managed by the server administrator."}
                    </p>
                    <p className="mt-2 text-xs text-muted-foreground">
                      Technical ID: {harness.id}
                    </p>
                  </details>
                ))}
              </>
            ) : (
              <p>
                Check this connection to discover its server harnesses. Tools
                installed on this device do not affect this catalog.
              </p>
            )}
            <p className="text-muted-foreground">
              If credentials are required, open Edit connection to enter them
              and test again.
            </p>
          </div>
        </SettingsOptionGroup>
      )}
      <SettingsOptionGroup
        title="Agent defaults"
        description="Used for new agents on this connection. Existing agents keep their own settings."
      >
        <div className="space-y-4 p-4">
          <ChoiceField
            label="Preferred harness"
            value={defaults.harness_id ?? ""}
            options={[
              { value: "", label: "No preference" },
              ...(harnesses ?? []).map((harness) => ({
                value: harness.id,
                label: harness.label,
              })),
              ...(defaults.harness_id &&
              !harnesses?.some((harness) => harness.id === defaults.harness_id)
                ? [
                    {
                      value: defaults.harness_id,
                      label: `${defaults.harness_id} — catalog not confirmed`,
                    },
                  ]
                : []),
            ]}
            onChange={(harness_id) =>
              setDefaults({ harness_id: harness_id || null, model: null })
            }
          />
          <ConnectionModelField
            key={`${connection.id}:${connection.revision}:${defaults.harness_id}`}
            connection={connection}
            harnessId={defaults.harness_id}
            value={defaults.model}
            onChange={(model) => setDefaults({ ...defaults, model })}
          />
          <Button
            type="button"
            disabled={pending}
            onClick={() => void saveDefaults()}
          >
            Save defaults
          </Button>
        </div>
      </SettingsOptionGroup>
      <div className="space-y-3">
        <Button
          type="button"
          variant="outline"
          disabled={pending}
          onClick={() => void inspectDelete()}
        >
          Remove connection
        </Button>
        {dependents && (
          <div className="space-y-2 text-sm">
            {dependents.length ? (
              <>
                <p>
                  This connection is used by these agents. Reassign stopped
                  agents before removing it.
                </p>
                <ul>
                  {dependents.map(({ name, pubkey }) => (
                    <li key={pubkey}>{name}</li>
                  ))}
                </ul>
              </>
            ) : (
              <>
                <p>
                  Remove this connection and its saved SSH credentials from this
                  device?
                </p>
                <Button
                  type="button"
                  variant="destructive"
                  disabled={pending}
                  onClick={() => void remove()}
                >
                  Confirm removal
                </Button>
              </>
            )}
            <Button
              type="button"
              variant="ghost"
              onClick={() => setDependents(null)}
            >
              Cancel
            </Button>
          </div>
        )}
      </div>
    </div>
  );
}
