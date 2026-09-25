import { useEffect, useId, useRef, useState } from "react";
import { Button } from "@/shared/ui/button";
import { Checkbox } from "@/shared/ui/checkbox";
import {
  listSshConfigHosts,
  resolveSshConfigHost,
  type SshEndpoint,
  type ConnectionCredentials,
} from "@/shared/api/tauriConnections";
import { ChoiceField, PathField, TextField } from "./ConnectionFields";
export function SshConnectionFields({
  endpoint,
  onChange,
  credentials,
  onCredentialsChange,
  onError,
}: {
  endpoint: SshEndpoint;
  onChange: (value: SshEndpoint) => void;
  credentials: ConnectionCredentials;
  onCredentialsChange: (value: ConnectionCredentials) => void;
  onError: (message: string) => void;
}) {
  const rememberId = useId();
  const [aliases, setAliases] = useState<string[]>([]);
  const [resolved, setResolved] = useState<{
    host: string | null;
    username: string | null;
    port: string | null;
  } | null>(null);
  const [loading, setLoading] = useState(false);
  const generation = useRef(0);
  const configPath = endpoint.mode === "config" ? endpoint.path : null;
  const alias = endpoint.mode === "config" ? endpoint.alias : null;
  // biome-ignore lint/correctness/useExhaustiveDependencies: Changing either input invalidates in-flight native SSH resolution.
  useEffect(() => {
    setResolved(null);
    generation.current++;
    return () => {
      generation.current++;
    };
  }, [configPath, alias]);
  async function loadConfig(path?: string) {
    const current = ++generation.current;
    setLoading(true);
    try {
      const result = await listSshConfigHosts(path);
      if (current !== generation.current) return;
      setAliases(result.aliases);
      onChange({ mode: "config", path: result.path, alias: "" });
    } catch (error) {
      if (current === generation.current) onError(String(error));
    } finally {
      if (current === generation.current) setLoading(false);
    }
  }
  async function resolve() {
    if (endpoint.mode !== "config") return;
    const current = ++generation.current;
    setLoading(true);
    try {
      const value = await resolveSshConfigHost(endpoint.path, endpoint.alias);
      if (current === generation.current) setResolved(value);
    } catch (error) {
      if (current === generation.current) onError(String(error));
    } finally {
      if (current === generation.current) setLoading(false);
    }
  }
  return (
    <div className="space-y-4">
      <ChoiceField
        label="Connection details"
        value={endpoint.mode}
        options={[
          { value: "manual", label: "Enter details" },
          { value: "config", label: "From SSH config" },
        ]}
        onChange={(mode) => {
          if (mode === "config") void loadConfig();
          else
            onChange({
              mode: "manual",
              host: resolved?.host ?? "",
              port: Number(resolved?.port) || 22,
              username: resolved?.username ?? "",
              authentication: { method: "agent" },
            });
        }}
      />
      {endpoint.mode === "manual" ? (
        <>
          <TextField
            label="Host"
            value={endpoint.host}
            onChange={(host) => onChange({ ...endpoint, host })}
          />
          <div className="grid gap-4 sm:grid-cols-2">
            <TextField
              label="Port"
              type="number"
              value={String(endpoint.port)}
              onChange={(port) => onChange({ ...endpoint, port: Number(port) })}
            />
            <TextField
              label="Username"
              value={endpoint.username}
              onChange={(username) => onChange({ ...endpoint, username })}
            />
          </div>
          <ChoiceField
            label="Authentication"
            value={endpoint.authentication.method}
            options={[
              { value: "password", label: "Password" },
              { value: "private_key", label: "Private key" },
              { value: "agent", label: "SSH agent" },
            ]}
            onChange={(method) =>
              onChange({
                ...endpoint,
                authentication:
                  method === "private_key"
                    ? { method, path: "" }
                    : method === "password"
                      ? { method }
                      : { method: "agent" },
              })
            }
          />
          {endpoint.authentication.method === "private_key" && (
            <PathField
              label="Private key file"
              value={endpoint.authentication.path}
              onError={onError}
              onChange={(path) =>
                onChange({
                  ...endpoint,
                  authentication: { method: "private_key", path },
                })
              }
            />
          )}
        </>
      ) : (
        <>
          <PathField
            label="SSH config file"
            value={endpoint.path}
            onError={onError}
            onChange={(path) => {
              onChange({ ...endpoint, path, alias: "" });
              setAliases([]);
            }}
          />
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={loading}
            onClick={() => void loadConfig(endpoint.path)}
          >
            Find connections
          </Button>
          <ChoiceField
            label="SSH alias"
            value={endpoint.alias}
            options={aliases.map((value) => ({ value, label: value }))}
            onChange={(alias) => onChange({ ...endpoint, alias })}
          />
          <Button
            type="button"
            variant="outline"
            size="sm"
            disabled={loading || !endpoint.alias}
            onClick={() => void resolve()}
          >
            Read connection details
          </Button>
          {resolved && (
            <p className="text-sm text-muted-foreground">
              {resolved.username}@{resolved.host}:{resolved.port}. Uses this SSH
              config, including its proxy and key settings.
            </p>
          )}
        </>
      )}
      {(endpoint.mode === "config" ||
        endpoint.authentication.method === "password") && (
        <TextField
          label="Password"
          type="password"
          value={credentials.password}
          onChange={(password) =>
            onCredentialsChange({ ...credentials, password })
          }
          description="Leave empty to use credentials already saved on this device."
        />
      )}
      {(endpoint.mode === "config" ||
        endpoint.authentication.method === "private_key") && (
        <TextField
          label="Key passphrase (optional)"
          type="password"
          value={credentials.passphrase}
          onChange={(passphrase) =>
            onCredentialsChange({ ...credentials, passphrase })
          }
        />
      )}
      {(endpoint.mode === "config" ||
        endpoint.authentication.method !== "agent") && (
        <>
          <label
            htmlFor={rememberId}
            className="flex items-center gap-2 text-sm"
          >
            <Checkbox
              id={rememberId}
              checked={credentials.remember}
              onCheckedChange={(remember) =>
                onCredentialsChange({
                  ...credentials,
                  remember: remember === true,
                })
              }
            />
            Remember credentials in this device’s secure storage
          </label>
          <p className="text-sm text-muted-foreground">
            {credentials.remember
              ? "Secrets are stored separately from connection settings."
              : "If credentials are not already saved, you will need to enter them for the next launch. A running server agent does not depend on them."}
          </p>
        </>
      )}
      {loading && (
        <p role="status" className="text-sm text-muted-foreground">
          Reading SSH configuration…
        </p>
      )}
    </div>
  );
}
