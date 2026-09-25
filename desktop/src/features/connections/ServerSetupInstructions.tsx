import { useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { invokeTauri } from "@/shared/api/tauri";
import { Button } from "@/shared/ui/button";
export function ServerSetupInstructions({ update }: { update: boolean }) {
  const [details, setDetails] = useState<{
    source: string | null;
    protocol: number;
    instructions_url: string | null;
  } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);
  async function show() {
    setPending(true);
    setError(null);
    try {
      setDetails(await invokeTauri("connection_setup_guidance"));
    } catch (error) {
      setError(String(error));
    } finally {
      setPending(false);
    }
  }
  return (
    <div className="space-y-2">
      <Button
        type="button"
        variant="outline"
        disabled={pending}
        onClick={() => void show()}
      >
        {update ? "Update server component" : "Set up server"}
      </Button>
      {details && (
        <>
          <p>
            Install the Connections host bundle matching this Desktop build on
            the SSH server. Nothing is installed by a connection check.
          </p>
          <p className="break-all">
            Host protocol: {details.protocol}. Source:{" "}
            {details.source ??
              "development build — use the repository host README"}
            .
          </p>
          {details.instructions_url && (
            <Button
              type="button"
              variant="link"
              onClick={() => {
                void openUrl(details.instructions_url as string).catch(
                  (error) => setError(String(error)),
                );
              }}
            >
              Open instructions for this build
            </Button>
          )}
          <p>After setup, return here and test the connection again.</p>
        </>
      )}
      {error && (
        <p role="alert" className="text-destructive">
          {error}
        </p>
      )}
    </div>
  );
}
