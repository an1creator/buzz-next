import { useWorkingFolderSettings } from "../useWorkingFolders";
import * as React from "react";
import { Input } from "@/shared/ui/input";
import { Button } from "@/shared/ui/button";
import type { WorkingFolderSettings } from "../useWorkingFolders";

export function AgentWorkingFolderField({
  settings,
  loading,
  error,
  retry,
  value,
  inherited,
  disabled,
  onChange,
}: {
  settings?: WorkingFolderSettings;
  loading: boolean;
  error: unknown;
  retry: () => void;
  value: string;
  inherited?: string;
  disabled: boolean;
  onChange: (value: string) => void;
}) {
  const id = React.useId();
  return (
    <div className="space-y-1.5">
      <label className="text-sm font-medium text-foreground" htmlFor={id}>
        Working folder
      </label>
      {loading ? (
        <p className="text-sm text-muted-foreground" role="status">
          Checking working folder support…
        </p>
      ) : error ? (
        <div className="text-sm text-destructive" role="alert">
          Could not check working folder support. Your saved setting is
          unchanged.{" "}
          <Button variant="ghost" type="button" onClick={retry}>
            Retry
          </Button>
        </div>
      ) : settings?.supported ? (
        <>
          <Input
            id={id}
            value={value}
            disabled={disabled}
            onChange={(event) => onChange(event.target.value)}
            placeholder={inherited || "Use the computer’s default folder"}
            aria-describedby={`${id}-help`}
            autoComplete="off"
            spellCheck={false}
          />
          <p id={`${id}-help`} className="text-xs text-muted-foreground">
            {settings.remote
              ? "Enter an absolute path on the server."
              : "Enter an absolute path on this computer."}{" "}
            Leave empty to inherit{inherited ? ` ${inherited}` : " the default"}
            . Channel settings can override this folder. Applies the next time
            you start the agent.
          </p>
        </>
      ) : (
        <p className="text-xs text-muted-foreground">
          This runtime does not advertise working folder support. Saved values
          are preserved.
        </p>
      )}
    </div>
  );
}

export function setWorkingFolderValue(
  previous: Record<string, string>,
  key: string,
  value: string,
) {
  const next = { ...previous };
  if (value.trim()) next[key] = value;
  else delete next[key];
  return next;
}

export function useAgentWorkingFolder({
  pubkey,
  open,
  envVars,
  inheritedEnvVars,
  disabled,
  setEnvVars,
}: {
  pubkey: string;
  open: boolean;
  envVars: Record<string, string>;
  inheritedEnvVars: Record<string, string>;
  disabled: boolean;
  setEnvVars: React.Dispatch<React.SetStateAction<Record<string, string>>>;
}) {
  const query = useWorkingFolderSettings(pubkey, undefined, open);
  const key = query.data?.defaultEnvKey;
  return {
    hiddenKeys: query.data?.supported && key ? [key] : [],
    field: (
      <AgentWorkingFolderField
        settings={query.data}
        loading={query.isPending}
        error={query.error}
        retry={() => {
          void query.refetch();
        }}
        value={key ? (envVars[key] ?? "") : ""}
        inherited={key ? inheritedEnvVars[key] : undefined}
        disabled={disabled}
        onChange={(value) => {
          if (key)
            setEnvVars((previous) =>
              setWorkingFolderValue(previous, key, value),
            );
        }}
      />
    ),
  };
}
