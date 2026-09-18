import type { RefObject } from "react";
import { useEffect, useRef } from "react";

export function useDismiss(
  open: boolean,
  host: RefObject<HTMLElement | null>,
  close: () => void,
): void {
  const closeRef = useRef(close);
  closeRef.current = close;
  useEffect(() => {
    if (!open) return undefined;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") closeRef.current();
    };
    const onPointer = (e: MouseEvent) => {
      const el = host.current;
      if (el && !el.contains(e.target as Node)) closeRef.current();
    };
    const onBlur = () => closeRef.current();
    document.addEventListener("keydown", onKey);
    document.addEventListener("mousedown", onPointer);
    window.addEventListener("blur", onBlur);
    return () => {
      document.removeEventListener("keydown", onKey);
      document.removeEventListener("mousedown", onPointer);
      window.removeEventListener("blur", onBlur);
    };
  }, [open, host]);
}
