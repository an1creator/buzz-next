import * as React from "react";
import { useQuery } from "@tanstack/react-query";
import { Folder } from "lucide-react";
import { invokeTauri } from "@/shared/api/tauri";
import { Button } from "@/shared/ui/button";
import { Input } from "@/shared/ui/input";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from "@/shared/ui/dialog";
import { useDeferredModalOpen } from "@/shared/ui/deferredModalOpen";
import {
  useWorkingFolderScope,
  useWorkingFolderSettings,
  type WorkingFolderTarget,
  type ChannelFolder,
} from "@/features/agents/useWorkingFolders";
import { PersonaDropdownField } from "@/features/agents/ui/PersonaDropdownField";
import { InfoFieldRow } from "./ChannelManagementSheetRows";

export function ChannelWorkingFolders({ channelId }: { channelId: string }) {
  const [open, setOpen] = React.useState(false);
  const [busy, setBusy] = React.useState(false);
  const { openNextFrame } = useDeferredModalOpen();
  const scope = useWorkingFolderScope();
  return (
    <>
      <InfoFieldRow
        label="Working folders"
        value="Configure for your agents"
        icon={Folder}
        onClick={() => openNextFrame(() => setOpen(true))}
        testId="channel-working-folders"
      />
      <Dialog
        open={open}
        onOpenChange={(value) => {
          if (!busy) setOpen(value);
        }}
      >
        {open ? (
          <WorkingFoldersContent
            key={`${scope.expectedRelayUrl}:${scope.expectedSignerPubkey}:${channelId}`}
            channelId={channelId}
            onBusy={setBusy}
            onClose={() => setOpen(false)}
          />
        ) : null}
      </Dialog>
    </>
  );
}

function WorkingFoldersContent({
  channelId,
  onClose,
  onBusy,
}: {
  channelId: string;
  onBusy: (busy: boolean) => void;
  onClose: () => void;
}) {
  const scope = useWorkingFolderScope();
  const [selected, setSelected] = React.useState("");
  const [dirty, setDirty] = React.useState(false);
  const [saving, setSaving] = React.useState(false);
  React.useEffect(() => () => onBusy(false), [onBusy]);
  const targets = useQuery({
    queryKey: [
      "working-folder-targets",
      scope.expectedRelayUrl,
      scope.expectedSignerPubkey,
    ],
    queryFn: () =>
      invokeTauri<WorkingFolderTarget[]>("working_folder_targets", scope),
    enabled: Boolean(scope.expectedRelayUrl && scope.expectedSignerPubkey),
    retry: false,
    refetchOnWindowFocus: false,
  });
  const target =
    targets.data?.find((t) => t.id === selected) ?? targets.data?.[0];
  return (
    <DialogContent className="max-w-lg">
      <DialogHeader>
        <DialogTitle>Working folders</DialogTitle>
        <DialogDescription>
          Choose where your agents work in this channel. These preferences
          belong to your identity on this installation.
        </DialogDescription>
      </DialogHeader>
      {targets.isLoading ? (
        <p role="status">Loading computers…</p>
      ) : targets.error ? (
        <p className="text-sm text-destructive" role="alert">
          Could not load computers.{" "}
          <Button
            type="button"
            variant="ghost"
            onClick={() => {
              void targets.refetch();
            }}
          >
            Retry
          </Button>
        </p>
      ) : target ? (
        <>
          <div className="space-y-1.5">
            <label
              htmlFor="working-folder-computer"
              className="text-sm font-medium"
            >
              Computer
            </label>
            <PersonaDropdownField
              id="working-folder-computer"
              placeholder="Choose a computer"
              value={target.id}
              onValueChange={setSelected}
              disabled={dirty || saving}
              options={(targets.data ?? []).map((t) => ({
                value: t.id,
                label: t.label,
              }))}
            />
          </div>
          {dirty ? (
            <p className="text-xs text-muted-foreground">
              Save or cancel before choosing another computer.
            </p>
          ) : null}
          <ChannelFolderEditor
            key={target.id}
            agentPubkey={target.agentPubkey}
            onDirty={setDirty}
            onBusy={(value) => {
              setSaving(value);
              onBusy(value);
            }}
            channelId={channelId}
            onClose={onClose}
          />
        </>
      ) : (
        <p className="text-sm text-muted-foreground">
          Create an agent first. Its computer will appear here.
        </p>
      )}
    </DialogContent>
  );
}

function ChannelFolderEditor({
  agentPubkey,
  channelId,
  onClose,
  onDirty,
  onBusy,
}: {
  agentPubkey: string;
  channelId: string;
  onClose: () => void;
  onDirty: (dirty: boolean) => void;
  onBusy: (busy: boolean) => void;
}) {
  const query = useWorkingFolderSettings(agentPubkey, channelId);
  const mounted = React.useRef(true);
  React.useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  const [draft, setDraft] = React.useState<string | null>(null);
  const [saving, setSaving] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  // Pin the loaded revision for the lifetime of this editor; a background refetch
  // must never silently replace an edited draft's compare-and-save revision.
  const [snapshot, setSnapshot] = React.useState<ChannelFolder | null>(null);
  React.useEffect(() => {
    if (!snapshot && !query.isFetching && query.data?.folder)
      setSnapshot(query.data.folder);
  }, [snapshot, query.data, query.isFetching]);
  const value = draft ?? snapshot?.path ?? "";
  const save = async () => {
    if (!query.data || !snapshot || saving) return;
    setSaving(true);
    onBusy(true);
    setError(null);
    try {
      await invokeTauri<ChannelFolder>("set_channel_working_folder", {
        ...query.scope,
        agentPubkey,
        targetId: query.data.targetId,
        channelId,
        path: value || null,
        revision: snapshot.revision,
      });
      if (mounted.current) onClose();
    } catch (error) {
      if (mounted.current) setError(String(error));
    } finally {
      if (mounted.current) {
        setSaving(false);
        onBusy(false);
      }
    }
  };
  return (
    <div className="space-y-3">
      {query.isPending || (!snapshot && query.isFetching) ? (
        <p className="text-sm text-muted-foreground" role="status">
          Checking working folder support…
        </p>
      ) : query.error ? (
        <p role="alert" className="text-sm text-destructive">
          Could not load working folders.{" "}
          <Button
            type="button"
            variant="ghost"
            onClick={() => {
              void query.refetch();
            }}
          >
            Retry
          </Button>
        </p>
      ) : !query.data?.supported ? (
        <p className="text-sm text-muted-foreground">
          Update this runtime to use channel working folders.
        </p>
      ) : (
        <>
          <div className="space-y-1.5">
            <label
              htmlFor="channel-working-folder"
              className="text-sm font-medium"
            >
              Working folder
            </label>
            <Input
              id="channel-working-folder"
              value={value}
              onChange={(e) => {
                setDraft(e.target.value);
                onDirty(e.target.value !== (snapshot?.path ?? ""));
              }}
              disabled={saving || !snapshot}
              placeholder="Use each agent’s default folder"
              aria-describedby="channel-working-folder-help"
              autoComplete="off"
              spellCheck={false}
            />
          </div>
          <p
            id="channel-working-folder-help"
            className="text-xs text-muted-foreground"
          >
            {query.data.remote
              ? "Enter an absolute path on the server."
              : "Enter an absolute path on this computer."}{" "}
            Leave empty to use each agent’s default folder. The folder must
            already exist.
          </p>
          <p className="text-xs text-muted-foreground">
            Applies the next time you start each agent. Running tasks keep their
            current folder; files are not moved.
          </p>
          {error ? (
            <p role="alert" className="text-sm text-destructive">
              {error}
            </p>
          ) : null}
        </>
      )}
      <div className="flex justify-end gap-2">
        <Button
          type="button"
          variant="outline"
          disabled={saving}
          onClick={onClose}
        >
          Cancel
        </Button>
        <Button
          type="button"
          disabled={
            saving ||
            !snapshot ||
            !query.data?.supported ||
            Boolean(query.error)
          }
          onClick={() => {
            void save();
          }}
        >
          {saving ? "Saving…" : "Save"}
        </Button>
      </div>
    </div>
  );
}
