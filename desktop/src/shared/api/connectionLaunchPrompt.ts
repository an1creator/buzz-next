import type { ConnectionProbeInput } from "./tauriConnections";
export type LaunchCredentialRequest = {
  pubkey: string;
  owner: string;
  relay: string;
  complete: (value: ConnectionProbeInput | null) => void;
};
let listener: ((request: LaunchCredentialRequest) => void) | null = null;
/** UI registration contains no saved credentials or community data. */
export function registerLaunchCredentialPrompt(
  value: (request: LaunchCredentialRequest) => void,
) {
  listener = value;
  return () => {
    if (listener === value) listener = null;
  };
}
export function requestLaunchCredentials(
  pubkey: string,
  owner: string,
  relay: string,
): Promise<ConnectionProbeInput> {
  return new Promise((resolve, reject) => {
    if (!listener) {
      reject(
        new Error(
          "Enter SSH credentials in this connection’s settings and try again.",
        ),
      );
      return;
    }
    let settled = false;
    function complete(value: ConnectionProbeInput | null) {
      if (settled) return;
      settled = true;
      if (value) resolve(value);
      else reject(new Error("SSH launch cancelled. No agent was started."));
    }
    listener({ pubkey, owner, relay, complete });
  });
}
