import { useId } from "react";
import type { ReactNode } from "react";
import { Input } from "@/shared/ui/input";
import { Button } from "@/shared/ui/button";
import { AgentDropdownSelect } from "@/features/agents/ui/agentConfigControls";
import { pickConnectionFile } from "@/shared/api/tauriConnections";
export function TextField({
  label,
  value,
  onChange,
  type = "text",
  disabled = false,
  description,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  type?: string;
  disabled?: boolean;
  description?: ReactNode;
}) {
  const id = useId();
  return (
    <div className="space-y-2">
      <label className="text-sm font-medium" htmlFor={id}>
        {label}
      </label>
      <Input
        id={id}
        value={value}
        type={type}
        disabled={disabled}
        onChange={(event) => onChange(event.target.value)}
        autoComplete={type === "password" ? "off" : undefined}
        aria-describedby={description ? `${id}-hint` : undefined}
      />
      {description && (
        <p id={`${id}-hint`} className="text-sm text-muted-foreground">
          {description}
        </p>
      )}
    </div>
  );
}
export function ChoiceField({
  label,
  value,
  onChange,
  options,
  disabled = false,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  options: { value: string; label: string; disabled?: boolean }[];
  disabled?: boolean;
}) {
  const id = useId();
  return (
    <div className="space-y-2">
      <label className="text-sm font-medium" htmlFor={id}>
        {label}
      </label>
      <AgentDropdownSelect
        id={id}
        value={value}
        options={options}
        onValueChange={onChange}
        disabled={disabled}
        selectedLabel={
          options.find((option) => option.value === value)?.label ??
          (value || undefined)
        }
      />
    </div>
  );
}
export function PathField({
  label,
  value,
  onChange,
  onError,
  directory = false,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  onError: (message: string) => void;
  directory?: boolean;
}) {
  return (
    <div className="space-y-2">
      <TextField label={label} value={value} onChange={onChange} />
      <Button
        type="button"
        variant="outline"
        size="sm"
        onClick={() =>
          void pickConnectionFile(directory)
            .then((path) => {
              if (path) onChange(path);
            })
            .catch((error) => onError(String(error)))
        }
      >
        {directory ? "Choose folder" : "Choose file"}
      </Button>
    </div>
  );
}
