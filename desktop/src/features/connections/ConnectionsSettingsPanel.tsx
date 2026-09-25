import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useIdentityQuery } from "@/shared/api/hooks";
import {
  listExecutionConnections,
  type ExecutionConnection,
  type ConnectionProbe,
} from "@/shared/api/tauriConnections";
import { Button } from "@/shared/ui/button";
import {
  SettingsOptionGroup,
  SettingsOptionRow,
} from "@/features/settings/ui/SettingsOptionGroup";
import { ConnectionEditor } from "./ConnectionEditor";
import { ConnectionDetail } from "./ConnectionDetail";
import { connectionStatus, savedCheck } from "./connectionPresentation";
type View =
  | { page: "list" }
  | { page: "edit"; connection?: ExecutionConnection }
  | {
      page: "detail";
      connection: ExecutionConnection;
      probe?: ConnectionProbe;
    };
export function ConnectionsSettingsPanel() {
  const identity = useIdentityQuery();
  return (
    <ConnectionsForIdentity
      key={identity.data?.pubkey ?? "anonymous"}
      owner={identity.data?.pubkey ?? "anonymous"}
    />
  );
}
function ConnectionsForIdentity({ owner }: { owner: string }) {
  const client = useQueryClient();
  const query = useQuery({
    queryKey: ["execution-connections", owner],
    queryFn: listExecutionConnections,
  });
  const [view, setView] = useState<View>({ page: "list" });
  function saved(connection: ExecutionConnection, probe?: ConnectionProbe) {
    client.setQueryData<ExecutionConnection[]>(
      ["execution-connections", owner],
      (previous) => [
        ...(previous ?? []).filter((item) => item.id !== connection.id),
        connection,
      ],
    );
    setView({ page: "detail", connection, probe });
  }
  if (view.page === "edit")
    return (
      <SettingsOptionGroup title="Connections">
        <div className="p-4">
          <ConnectionEditor
            initial={view.connection}
            existingLocal={query.data?.find(
              (item) => item.target.type === "local",
            )}
            onUseExisting={(connection) =>
              setView({ page: "detail", connection })
            }
            onSaved={saved}
            onCancel={() =>
              view.connection
                ? setView({ page: "detail", connection: view.connection })
                : setView({ page: "list" })
            }
          />
        </div>
      </SettingsOptionGroup>
    );
  if (view.page === "detail")
    return (
      <ConnectionDetail
        key={`${view.connection.id}:${view.connection.revision}`}
        connection={view.connection}
        initialProbe={view.probe}
        onSaved={saved}
        onEdit={() => setView({ page: "edit", connection: view.connection })}
        onBack={() => setView({ page: "list" })}
        onDeleted={() => {
          void client.invalidateQueries({
            queryKey: ["execution-connections", owner],
          });
          setView({ page: "list" });
        }}
      />
    );
  return (
    <SettingsOptionGroup
      title="Connections"
      description="Choose where your agents run."
      headerAction={
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={() => setView({ page: "edit" })}
        >
          Add connection
        </Button>
      }
      data-testid="settings-connections"
    >
      {query.isPending ? (
        <p role="status" className="p-4 text-sm text-muted-foreground">
          Loading connections…
        </p>
      ) : query.isError ? (
        <div className="space-y-2 p-4">
          <p role="alert" className="text-sm text-destructive">
            Could not load connections. {String(query.error)}
          </p>
          <Button
            type="button"
            variant="outline"
            onClick={() => void query.refetch()}
          >
            Retry
          </Button>
        </div>
      ) : query.data.length === 0 ? (
        <div className="space-y-2 p-4">
          <p className="text-sm font-medium">No connections yet</p>
          <p className="text-sm text-muted-foreground">
            Add this device or an SSH server to run your agents. Conversations
            remain available without a connection.
          </p>
        </div>
      ) : (
        query.data.map((connection) => (
          <SettingsOptionRow key={connection.id}>
            <div className="min-w-0">
              <p className="break-words font-medium">{connection.name}</p>
              <p className="text-sm text-muted-foreground">
                {connection.target.type === "local" ? "This device" : "SSH"} ·{" "}
                {connectionStatus(savedCheck(connection)?.outcome)}
              </p>
            </div>
            <Button
              type="button"
              variant="outline"
              aria-label={`Open ${connection.name}`}
              onClick={() => setView({ page: "detail", connection })}
            >
              Open
            </Button>
          </SettingsOptionRow>
        ))
      )}
    </SettingsOptionGroup>
  );
}
