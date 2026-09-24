import { Button } from "@/shared/ui/button";

export function AgentEditDialogFooter({
  saving,
  avatarPending,
  canSubmit,
  onCancel,
  onSave,
}: {
  saving: boolean;
  avatarPending: boolean;
  canSubmit: boolean;
  onCancel: () => void;
  onSave: () => void;
}) {
  return (
    <div className="flex w-full items-center justify-end gap-2">
      <Button
        disabled={saving || avatarPending}
        onClick={onCancel}
        type="button"
        variant="outline"
      >
        Cancel
      </Button>
      <Button
        data-testid="edit-agent-dialog-submit"
        disabled={!canSubmit}
        onClick={onSave}
        type="button"
      >
        {saving ? "Saving..." : "Save changes"}
      </Button>
    </div>
  );
}
