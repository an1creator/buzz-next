import { useState } from "react";
import { Button } from "@/shared/ui/button";
import {
  saveExecutionConnection,
  type ExecutionConnection,
  type ConnectionCredentials,
  type ConnectionProbe,
} from "@/shared/api/tauriConnections";
import { TextField } from "./ConnectionFields";
import { SshConnectionFields } from "./SshConnectionFields";
import { ConnectionCheckResult } from "./ConnectionCheckResult";
import { emptyConnection } from "./connectionPresentation";
import { useConnectionProbe } from "./useConnectionProbe";

/** Shared inline flow: parent replaces its body instead of stacking modal dialogs. */
export function ConnectionEditor({
  initial,
  existingLocal,
  onSaved,
  onCancel,
  onUseExisting,
}: {
  initial?: ExecutionConnection;
  existingLocal?: ExecutionConnection;
  onSaved: (value: ExecutionConnection, probe?: ConnectionProbe) => void;
  onCancel: () => void;
  onUseExisting?: (value: ExecutionConnection) => void;
}) {
  const [draft, setDraft] = useState<ExecutionConnection | null>(
    initial ?? null,
  );
  const [credentials, setCredentials] = useState<ConnectionCredentials>({
    password: "",
    passphrase: "",
    remember: false,
  });
  const [credentialsChanged, setCredentialsChanged] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const probe = useConnectionProbe();
  function update(value: ExecutionConnection) {
    probe.invalidate();
    setError(null);
    setDraft(value);
  }
  function check(approved: string[] = []) {
    if (draft)
      void probe.check(draft, {
        password: credentials.password,
        passphrase: credentials.passphrase,
        approved_host_prompts: approved,
      });
  }
  async function save() {
    if (!draft) return;
    setSaving(true);
    setError(null);
    try {
      const saved = await saveExecutionConnection(
        draft,
        credentialsChanged ? credentials : undefined,
        probe.result?.ticket,
      );
      setCredentials({ password: "", passphrase: "", remember: false });
      onSaved(saved, probe.result ?? undefined);
    } catch (error) {
      setError(String(error));
    } finally {
      setSaving(false);
    }
  }
  if (!draft)
    return (
      <div className="space-y-4">
        <h3 className="font-semibold">Add connection</h3>
        <p className="text-sm text-muted-foreground">
          Where should your agents run?
        </p>
        <div className="grid gap-3 sm:grid-cols-2">
          <Button
            type="button"
            variant="outline"
            onClick={() =>
              existingLocal
                ? onUseExisting?.(existingLocal)
                : setDraft(emptyConnection("local"))
            }
          >
            This device{existingLocal ? " — already added" : ""}
          </Button>
          <Button
            type="button"
            variant="outline"
            onClick={() => setDraft(emptyConnection("ssh"))}
          >
            SSH server
          </Button>
        </div>
        <Button type="button" variant="ghost" onClick={onCancel}>
          Cancel
        </Button>
      </div>
    );
  return (
    <div className="space-y-5" data-testid="connection-editor">
      <div>
        <h3 className="font-semibold">
          {initial ? "Edit connection" : "Add connection"}
        </h3>
        <p className="text-sm text-muted-foreground">
          {draft.target.type === "local"
            ? "Run agents on this device. Local agents depend on this computer and Buzz staying available."
            : "Launch agents on a server. They continue running when Buzz is closed."}
        </p>
      </div>
      <fieldset className="space-y-4" disabled={saving || probe.pending}>
        <TextField
          label="Name"
          value={draft.name}
          onChange={(name) => update({ ...draft, name })}
        />
        {draft.target.type === "ssh" && (
          <SshConnectionFields
            endpoint={draft.target.endpoint}
            onChange={(endpoint) =>
              update({ ...draft, target: { type: "ssh", endpoint } })
            }
            credentials={credentials}
            onCredentialsChange={(value) => {
              probe.invalidate();
              setCredentials(value);
              setCredentialsChanged(true);
            }}
            onError={setError}
          />
        )}
      </fieldset>
      {(error || probe.error) && (
        <p role="alert" className="text-sm text-destructive">
          {error || probe.error}
        </p>
      )}
      {probe.pending && (
        <p role="status" className="text-sm text-muted-foreground">
          {draft.target.type === "ssh"
            ? "Checking SSH access, server compatibility, and harnesses…"
            : "Checking installed harnesses…"}
        </p>
      )}
      {probe.result && (
        <ConnectionCheckResult
          result={probe.result}
          onTrust={(prompt) => check([prompt])}
        />
      )}
      <div className="flex flex-wrap gap-2">
        {probe.pending ? (
          <Button type="button" variant="outline" onClick={probe.invalidate}>
            Cancel check
          </Button>
        ) : (
          <Button
            type="button"
            variant="outline"
            disabled={saving}
            onClick={() => check()}
          >
            Test connection
          </Button>
        )}
        <Button
          type="button"
          disabled={saving || probe.pending || !draft.name.trim()}
          onClick={() => void save()}
        >
          {saving ? "Saving…" : "Save connection"}
        </Button>
        <Button
          type="button"
          variant="ghost"
          disabled={saving}
          onClick={() => {
            probe.invalidate();
            onCancel();
          }}
        >
          Cancel
        </Button>
      </div>
      <p className="text-sm text-muted-foreground">
        Testing does not save settings or install software. You can save a
        connection before it is ready.
      </p>
    </div>
  );
}
