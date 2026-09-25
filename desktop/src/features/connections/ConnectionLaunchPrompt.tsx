import { useEffect, useRef, useState } from "react";
import {
  registerLaunchCredentialPrompt,
  type LaunchCredentialRequest,
} from "@/shared/api/connectionLaunchPrompt";
import { useIdentityQuery } from "@/shared/api/hooks";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from "@/shared/ui/dialog";
import { Button } from "@/shared/ui/button";
import { TextField } from "./ConnectionFields";
export function ConnectionLaunchPrompt() {
  const identity = useIdentityQuery();
  const [request, setRequest] = useState<LaunchCredentialRequest | null>(null);
  const active = useRef<LaunchCredentialRequest | null>(null);
  const [password, setPassword] = useState("");
  const [passphrase, setPassphrase] = useState("");
  useEffect(() => {
    setRequest(null);
    setPassword("");
    setPassphrase("");
    const unregister = registerLaunchCredentialPrompt((next) => {
      if (active.current || next.owner !== identity.data?.pubkey) {
        next.complete(null);
        return;
      }
      active.current = next;
      setRequest(next);
      setPassword("");
      setPassphrase("");
    });
    return () => {
      unregister();
      active.current?.complete(null);
      active.current = null;
    };
  }, [identity.data?.pubkey]);
  function finish(start: boolean) {
    const current = active.current;
    active.current = null;
    setRequest(null);
    current?.complete(
      start ? { password, passphrase, approved_host_prompts: [] } : null,
    );
    setPassword("");
    setPassphrase("");
  }
  return (
    <Dialog
      open={request !== null}
      onOpenChange={(open) => {
        if (!open) finish(false);
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>SSH sign-in required</DialogTitle>
          <DialogDescription>
            Enter the password or key passphrase for this connection. It is used
            only for this operation and is not saved.
          </DialogDescription>
        </DialogHeader>
        <form
          className="space-y-4"
          onSubmit={(event) => {
            event.preventDefault();
            finish(true);
          }}
        >
          <TextField
            label="Password"
            type="password"
            value={password}
            onChange={setPassword}
          />
          <TextField
            label="Key passphrase"
            type="password"
            value={passphrase}
            onChange={setPassphrase}
          />
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => finish(false)}
            >
              Cancel
            </Button>
            <Button type="submit" disabled={!password && !passphrase}>
              Continue
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
