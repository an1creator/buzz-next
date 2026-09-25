import type { ConnectionProbe } from "@/shared/api/tauriConnections";
import { Button } from "@/shared/ui/button";
import { ServerSetupInstructions } from "./ServerSetupInstructions";
import { connectionStatus } from "./connectionPresentation";
export function ConnectionCheckResult({
  result,
  onTrust,
}: {
  result: ConnectionProbe;
  onTrust: (prompt: string) => void;
}) {
  return (
    <div
      className="space-y-3 rounded-lg border p-4 text-sm"
      role="status"
      aria-live="polite"
    >
      <p>{connectionStatus(result.check.outcome)}</p>
      {result.check.outcome.status === "host_trust_required" &&
        result.trust_prompt && (
          <>
            <p className="break-all font-mono">
              {result.check.outcome.fingerprint}
            </p>
            <p>
              Verify this fingerprint with the server administrator before
              trusting it.
            </p>
            <Button
              type="button"
              variant="outline"
              onClick={() => onTrust(result.trust_prompt ?? "")}
            >
              Trust this server and continue
            </Button>
          </>
        )}
      {result.check.outcome.status === "host_key_changed" && (
        <p>
          Buzz has not changed your known-hosts file. Verify the new identity
          before updating your SSH trust settings.
        </p>
      )}
      {["setup_required", "update_required"].includes(
        result.check.outcome.status,
      ) && (
        <ServerSetupInstructions
          update={result.check.outcome.status === "update_required"}
        />
      )}
      {result.harnesses.length > 0 && (
        <ul className="space-y-2">
          {result.harnesses.map((harness) => (
            <li key={harness.id}>
              <span className="font-medium">{harness.label}</span> —{" "}
              {harness.availability !== "available"
                ? "Setup required"
                : harness.authStatus.status === "logged_in" ||
                    harness.authStatus.status === "not_applicable"
                  ? "Ready"
                  : "Sign-in required"}
            </li>
          ))}
        </ul>
      )}
      {result.check.default_directory && (
        <p className="break-all text-muted-foreground">
          Automatic working directory: {result.check.default_directory}
        </p>
      )}
    </div>
  );
}
