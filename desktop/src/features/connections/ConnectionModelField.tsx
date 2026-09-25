import { useEffect, useRef, useState } from "react";
import type { AgentModelsResponse } from "@/shared/api/types";
import {
  cancelConnectionCheck,
  getConnectionModels,
  type ExecutionConnection,
  type ConnectionProbeInput,
} from "@/shared/api/tauriConnections";
import { Button } from "@/shared/ui/button";
import { ChoiceField, TextField } from "./ConnectionFields";

/** The parent keys this view by owner, connection revision and harness. */
export function ConnectionModelField({
  connection,
  harnessId,
  value,
  onChange,
  input,
}: {
  connection: ExecutionConnection;
  harnessId: string | null;
  value: string | null;
  onChange: (value: string | null) => void;
  input?: ConnectionProbeInput;
}) {
  const [catalog, setCatalog] = useState<AgentModelsResponse | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);
  const [custom, setCustom] = useState(false);
  const [openRequest, setOpenRequest] = useState(0);
  const operation = useRef<string | null>(null);
  useEffect(
    () => () => {
      const id = operation.current;
      operation.current = null;
      if (id)
        void cancelConnectionCheck(id).catch((error) =>
          console.warn("Model check cancellation failed", error),
        );
    },
    [],
  );
  async function discover() {
    if (!harnessId || pending) return;
    const id = crypto.randomUUID();
    operation.current = id;
    setPending(true);
    setError(null);
    try {
      const result = await getConnectionModels(
        connection,
        harnessId,
        input ?? { password: "", passphrase: "", approved_host_prompts: [] },
        id,
      );
      if (operation.current === id) {
        setCatalog(result);
        if (result.models.length > 0) setOpenRequest((request) => request + 1);
      }
    } catch (error) {
      if (operation.current === id) setError(String(error));
    } finally {
      if (operation.current === id) {
        operation.current = null;
        setPending(false);
      }
    }
  }
  const labels = new Map<string, number>();
  for (const model of catalog?.models ?? []) {
    const label = model.name ?? model.id;
    labels.set(label, (labels.get(label) ?? 0) + 1);
  }
  function cancel() {
    const id = operation.current;
    operation.current = null;
    setPending(false);
    if (id)
      void cancelConnectionCheck(id).catch((error) =>
        setError(`Could not cancel model check: ${String(error)}`),
      );
  }
  return (
    <div className="space-y-2">
      <ChoiceField
        label="Model"
        value={value ?? ""}
        options={[
          { value: "", label: "Harness default" },
          ...(catalog?.models ?? []).map((model) => ({
            value: model.id,
            label:
              (labels.get(model.name ?? model.id) ?? 0) > 1
                ? `${model.name ?? model.id} · ${model.id}`
                : (model.name ?? model.id),
          })),
          ...(value && !catalog?.models.some((model) => model.id === value)
            ? [{ value, label: `${value} — saved selection` }]
            : []),
        ]}
        onChange={(model) => onChange(model || null)}
        openRequest={openRequest}
        searchable
      />
      <div className="flex flex-wrap gap-2">
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={!harnessId || pending}
          onClick={() => void discover()}
        >
          {pending ? "Loading models…" : "Load models"}
        </Button>
        {pending && (
          <Button type="button" variant="outline" size="sm" onClick={cancel}>
            Cancel model check
          </Button>
        )}
        <Button
          type="button"
          variant="ghost"
          size="sm"
          onClick={() => setCustom(!custom)}
          aria-expanded={custom}
        >
          Custom model
        </Button>
      </div>
      {catalog && catalog.models.length > 0 && (
        <p role="status" className="text-sm text-muted-foreground">
          {catalog.models.length} models loaded. Choose one from Model, or keep
          the harness default.
        </p>
      )}
      {custom && (
        <TextField
          label="Model ID"
          value={value ?? ""}
          onChange={(model) => onChange(model || null)}
          description="Use an exact model ID supported by this harness, or leave empty for its default."
        />
      )}
      {catalog?.models.length === 0 && (
        <p role="status" className="text-sm text-muted-foreground">
          This harness did not report a model list. Use its default or a known
          model ID.
        </p>
      )}
      {error && (
        <p role="alert" className="text-sm text-destructive">
          {error}
        </p>
      )}
    </div>
  );
}
