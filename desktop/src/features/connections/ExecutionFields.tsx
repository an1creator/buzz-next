import { useCallback, useEffect, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useIdentityQuery } from "@/shared/api/hooks";
import {
  listExecutionConnections,
  validateExecutionDirectory,
  cancelConnectionCheck,
  type ExecutionConnection,
  type AgentExecution,
} from "@/shared/api/tauriConnections";
import { Button } from "@/shared/ui/button";
import { ChoiceField, PathField, TextField } from "./ConnectionFields";
import { ConnectionEditor } from "./ConnectionEditor";
import { ConnectionCheckResult } from "./ConnectionCheckResult";
import { ConnectionModelField } from "./ConnectionModelField";
import { useConnectionProbe } from "./useConnectionProbe";

export function ExecutionFields({
  value,
  onChange,
  onEditingChange,
  onLocationChange,
}: {
  value: AgentExecution | null;
  onChange: (value: AgentExecution) => void;
  onEditingChange?: (editing: boolean) => void;
  onLocationChange?: (location: "local" | "remote") => void;
}) {
  const identity = useIdentityQuery();
  const query = useQuery({
    queryKey: ["execution-connections", identity.data?.pubkey ?? "anonymous"],
    queryFn: listExecutionConnections,
  });
  const [editing, setEditing] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const connection = query.data?.find(
    (item) => item.id === value?.connection_id,
  );
  function select(next: ExecutionConnection) {
    onLocationChange?.(next.target.type === "local" ? "local" : "remote");
    if (value && value.connection_id !== next.id)
      setNotice(
        "Connection changed. Check the harness and model; working directory is now Automatic.",
      );
    onChange({
      connection_id: next.id,
      harness_id: value?.harness_id ?? next.defaults.harness_id,
      model: value?.model ?? next.defaults.model,
      directory: { mode: "automatic" },
    });
  }
  function finish() {
    setEditing(false);
    onEditingChange?.(false);
  }
  if (editing)
    return (
      <ConnectionEditor
        existingLocal={query.data?.find((item) => item.target.type === "local")}
        onCancel={finish}
        onUseExisting={(connection) => {
          select(connection);
          finish();
        }}
        onSaved={(connection) => {
          void query.refetch();
          select(connection);
          finish();
        }}
      />
    );
  return (
    <section className="space-y-4" aria-label="Execution">
      <div>
        <h3 className="text-sm font-semibold">Execution</h3>
        <p className="text-sm text-muted-foreground">
          Where this agent runs. Saved changes apply on its next start.
        </p>
      </div>
      {query.isPending ? (
        <p role="status" className="text-sm">
          Loading connections…
        </p>
      ) : query.isError ? (
        <div>
          <p role="alert" className="text-sm text-destructive">
            Could not load connections.
          </p>
          <Button
            type="button"
            variant="outline"
            onClick={() => void query.refetch()}
          >
            Retry
          </Button>
        </div>
      ) : (
        <ChoiceField
          label="Connection"
          value={value?.connection_id ?? ""}
          options={[
            { value: "", label: "Choose a connection", disabled: true },
            ...query.data.map((item) => ({ value: item.id, label: item.name })),
            ...(value && !connection
              ? [
                  {
                    value: value.connection_id,
                    label: "Saved connection unavailable",
                  },
                ]
              : []),
          ]}
          onChange={(id) => {
            const next = query.data.find((item) => item.id === id);
            if (next) select(next);
          }}
        />
      )}
      <Button
        type="button"
        variant="outline"
        size="sm"
        onClick={() => {
          setEditing(true);
          onEditingChange?.(true);
        }}
      >
        Add connection
      </Button>
      {notice && (
        <p role="status" className="text-sm text-muted-foreground">
          {notice}
        </p>
      )}
      {value && connection && (
        <ConnectionExecutionFields
          key={`${identity.data?.pubkey}:${connection.id}:${connection.revision}`}
          value={value}
          connection={connection}
          onChange={onChange}
        />
      )}
    </section>
  );
}
function ConnectionExecutionFields({
  value,
  connection,
  onChange,
}: {
  value: AgentExecution;
  connection: ExecutionConnection;
  onChange: (value: AgentExecution) => void;
}) {
  const probe = useConnectionProbe();
  const [password, setPassword] = useState("");
  const [passphrase, setPassphrase] = useState("");
  const [directoryResult, setDirectoryResult] = useState<string | null>(null);
  const [directoryPending, setDirectoryPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const directoryGeneration = useRef(0);
  const directoryOperation = useRef<string | null>(null);
  const cancelDirectory = useCallback(() => {
    directoryGeneration.current++;
    const id = directoryOperation.current;
    directoryOperation.current = null;
    if (id)
      void cancelConnectionCheck(id).catch(() => {
        // Native checks remain bounded if cancellation IPC is unavailable.
      });
  }, []);
  const directorySelection = useRef(value.directory);
  useEffect(() => {
    directorySelection.current = value.directory;
    cancelDirectory();
    setDirectoryResult(null);
    setError(null);
    setDirectoryPending(false);
    return () => {
      cancelDirectory();
    };
  }, [value.directory, cancelDirectory]);
  // Discovery is read-only; each mounted connection owns its result and cancellation.
  const check = probe.check;
  useEffect(() => {
    void check(connection, {
      password: "",
      passphrase: "",
      approved_host_prompts: [],
    });
  }, [check, connection]);
  const harnesses = probe.result?.harnesses;
  async function checkDirectory() {
    cancelDirectory();
    const generation = ++directoryGeneration.current;
    const operationId = crypto.randomUUID();
    directoryOperation.current = operationId;
    const selection = value.directory;
    setError(null);
    setDirectoryResult(null);
    setDirectoryPending(true);
    try {
      const result = await validateExecutionDirectory(
        connection,
        value.directory,
        { password, passphrase, approved_host_prompts: [] },
        operationId,
      );
      if (
        generation === directoryGeneration.current &&
        selection === directorySelection.current
      )
        setDirectoryResult(result.path);
    } catch (error) {
      if (
        generation === directoryGeneration.current &&
        selection === directorySelection.current
      )
        setError(String(error));
    } finally {
      if (
        generation === directoryGeneration.current &&
        selection === directorySelection.current
      ) {
        directoryOperation.current = null;
        setDirectoryPending(false);
      }
    }
  }
  return (
    <div className="space-y-4">
      {probe.pending && (
        <p role="status" className="text-sm">
          Discovering harnesses on {connection.name}…
        </p>
      )}
      {probe.error && (
        <p role="alert" className="text-sm text-destructive">
          {probe.error}
        </p>
      )}
      {probe.result && probe.result.check.outcome.status !== "ready" && (
        <ConnectionCheckResult
          result={probe.result}
          onTrust={(prompt) =>
            void probe.check(connection, {
              password,
              passphrase,
              approved_host_prompts: [prompt],
            })
          }
        />
      )}
      {connection.target.type === "ssh" &&
        probe.result?.check.outcome.status === "authentication_required" && (
          <>
            <TextField
              label="SSH password"
              type="password"
              value={password}
              onChange={setPassword}
            />
            <TextField
              label="Key passphrase"
              type="password"
              value={passphrase}
              onChange={setPassphrase}
            />
          </>
        )}
      <Button
        type="button"
        variant="outline"
        size="sm"
        disabled={probe.pending}
        onClick={() =>
          void probe.check(connection, {
            password,
            passphrase,
            approved_host_prompts: [],
          })
        }
      >
        Check harnesses
      </Button>
      <ChoiceField
        label="Harness"
        value={value.harness_id ?? ""}
        options={[
          { value: "", label: "Choose a harness", disabled: true },
          ...(harnesses ?? []).map((item) => ({
            value: item.id,
            label: item.label,
          })),
          ...(value.harness_id &&
          !harnesses?.some((item) => item.id === value.harness_id)
            ? [
                {
                  value: value.harness_id,
                  label: `${value.harness_id} — catalog not confirmed`,
                },
              ]
            : []),
        ]}
        onChange={(harness_id) =>
          onChange({ ...value, harness_id, model: null })
        }
      />
      <ConnectionModelField
        key={value.harness_id ?? "none"}
        connection={connection}
        harnessId={value.harness_id}
        value={value.model}
        input={{ password, passphrase, approved_host_prompts: [] }}
        onChange={(model) => onChange({ ...value, model })}
      />
      <div className="space-y-2">
        <ChoiceField
          label="Working directory"
          value={value.directory.mode}
          options={[
            { value: "automatic", label: "Automatic" },
            { value: "explicit", label: "Choose a folder" },
          ]}
          onChange={(mode) => {
            setDirectoryResult(null);
            onChange({
              ...value,
              directory:
                mode === "automatic"
                  ? { mode }
                  : { mode: "explicit", path: "" },
            });
          }}
        />
        {value.directory.mode === "automatic" ? (
          <p className="break-all text-sm text-muted-foreground">
            {probe.result?.check.default_directory
              ? `Uses ${probe.result.check.default_directory} on ${connection.name}`
              : "The directory will be resolved on this connection before start."}
          </p>
        ) : connection.target.type === "local" ? (
          <PathField
            directory
            label="Working directory path"
            value={value.directory.path}
            onError={setError}
            onChange={(path) => {
              setDirectoryResult(null);
              onChange({ ...value, directory: { mode: "explicit", path } });
            }}
          />
        ) : (
          <TextField
            label="Absolute server path"
            value={value.directory.path}
            onChange={(path) => {
              setDirectoryResult(null);
              onChange({ ...value, directory: { mode: "explicit", path } });
            }}
            description="For example: /home/user/projects. This folder must already exist on the server."
          />
        )}
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={directoryPending}
          onClick={() => void checkDirectory()}
        >
          {directoryPending ? "Checking folder…" : "Check folder"}
        </Button>
        {directoryPending && (
          <Button
            type="button"
            variant="ghost"
            onClick={() => {
              cancelDirectory();
              setDirectoryPending(false);
            }}
          >
            Cancel folder check
          </Button>
        )}
        {directoryResult && (
          <p role="status" className="break-all text-sm">
            Verified: {directoryResult}
          </p>
        )}
        {error && (
          <p role="alert" className="text-sm text-destructive">
            {error}
          </p>
        )}
      </div>
      <p className="text-sm text-muted-foreground">
        You can save an agent before its harness is ready. Start checks the
        connection, harness, and folder again.
      </p>
    </div>
  );
}
