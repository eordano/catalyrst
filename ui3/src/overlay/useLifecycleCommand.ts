import { useEffect, useRef, useState } from "react";
import { lifecycleCommand, type LifecycleCommand } from "./lifecycleCommand";

export function useLifecycleCommand() {
  const controller = useRef<AbortController | null>(null);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => () => controller.current?.abort(), []);
  const run = (command: LifecycleCommand) => {
    if (controller.current && !controller.current.signal.aborted) return;
    const current = new AbortController();
    controller.current = current;
    setPending(true);
    setError(null);
    void lifecycleCommand(command, { signal: current.signal }).catch((error: unknown) => {
      if (!current.signal.aborted) setError(error instanceof Error ? error.message : String(error));
    }).finally(() => {
      if (current.signal.aborted) return;
      controller.current = null;
      setPending(false);
    });
  };
  return { run, pending, error };
}
