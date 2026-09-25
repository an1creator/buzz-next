import { useCallback, useEffect, useRef, useState } from "react";
import {
  cancelConnectionCheck,
  testExecutionConnection,
  type ExecutionConnection,
  type ConnectionProbe,
  type ConnectionProbeInput,
} from "@/shared/api/tauriConnections";
/** Results belong to this view and generation, never a global harness cache. */
export function useConnectionProbe() {
  const [result, setResult] = useState<ConnectionProbe | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);
  const operation = useRef<string | null>(null);
  const generation = useRef(0);
  const invalidate = useCallback(() => {
    generation.current++;
    const id = operation.current;
    operation.current = null;
    if (id)
      void cancelConnectionCheck(id).catch(() => {
        /* Check is bounded even if cancellation IPC fails. */
      });
    setPending(false);
    setResult(null);
    setError(null);
  }, []);
  useEffect(
    () => () => {
      generation.current++;
      const id = operation.current;
      if (id) void cancelConnectionCheck(id).catch(() => {});
    },
    [],
  );
  const check = useCallback(
    async (connection: ExecutionConnection, input: ConnectionProbeInput) => {
      invalidate();
      const current = generation.current;
      const id = crypto.randomUUID();
      operation.current = id;
      setPending(true);
      try {
        const value = await testExecutionConnection(connection, input, id);
        if (generation.current === current) setResult(value);
        return generation.current === current ? value : null;
      } catch (error) {
        if (generation.current === current) setError(String(error));
        return null;
      } finally {
        if (generation.current === current) {
          operation.current = null;
          setPending(false);
        }
      }
    },
    [invalidate],
  );
  return { result, error, pending, check, invalidate };
}
