import { useEffect, useRef } from "react";
import type { EngineInput, EngineInputOptions } from "./engineInput";
import { createEngineInput } from "./engineInput";

export function useEngineInput(options: EngineInputOptions = {}): EngineInput {
  const ref = useRef<EngineInput | null>(null);
  if (ref.current === null) ref.current = createEngineInput(options);
  const input = ref.current;

  useEffect(() => {
    input.configure(options);
  });

  useEffect(() => {
    input.activate();
    const release = () => input.releaseAll();
    const onVisibility = () => {
      if (document.visibilityState === "hidden") release();
    };
    window.addEventListener("blur", release);
    window.addEventListener("pagehide", release);
    document.addEventListener("visibilitychange", onVisibility);
    return () => {
      window.removeEventListener("blur", release);
      window.removeEventListener("pagehide", release);
      document.removeEventListener("visibilitychange", onVisibility);
      input.dispose();
    };
  }, [input]);

  return input;
}
