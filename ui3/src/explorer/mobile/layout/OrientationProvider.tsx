import type { ReactNode } from "react";
import { createContext, useCallback, useContext, useEffect, useMemo, useState } from "react";
import type { Orientation } from "./orientation";
import { useViewportOrientation } from "./orientation";
import "./viewport.css";
import "./layout.css";

export type SafeAreaEmulation = "ios" | "android";

export type OrientationState = {
  orientation: Orientation;
  viewport: Orientation;
  declared: Orientation | null;
  declare: (orientation: Orientation) => () => void;
};

type Declaration = { id: number; orientation: Orientation };

const OrientationContext = createContext<OrientationState | null>(null);

let nextDeclarationId = 0;

export type OrientationProviderProps = {
  children: ReactNode;
  orientation?: Orientation;
  safeAreaEmulation?: SafeAreaEmulation | null;
  stampDocument?: boolean;
  className?: string;
};

export function OrientationProvider({
  children,
  orientation,
  safeAreaEmulation = null,
  stampDocument = true,
  className,
}: OrientationProviderProps) {
  const viewport = useViewportOrientation();
  const [stack, setStack] = useState<readonly Declaration[]>([]);

  const declare = useCallback((next: Orientation): (() => void) => {
    const id = ++nextDeclarationId;
    setStack((current) => [...current, { id, orientation: next }]);
    let released = false;
    return () => {
      if (released) return;
      released = true;
      setStack((current) => current.filter((entry) => entry.id !== id));
    };
  }, []);

  const top = stack.length > 0 ? stack[stack.length - 1] : undefined;
  const declared = orientation ?? top?.orientation ?? null;
  const resolved = declared ?? viewport;

  useEffect(() => {
    if (!stampDocument || typeof document === "undefined") return undefined;
    const root = document.documentElement;
    const previous = root.getAttribute("data-orientation");
    root.setAttribute("data-orientation", resolved);
    return () => {
      if (previous === null) root.removeAttribute("data-orientation");
      else root.setAttribute("data-orientation", previous);
    };
  }, [stampDocument, resolved]);

  useEffect(() => {
    if (!stampDocument || typeof document === "undefined") return undefined;
    const root = document.documentElement;
    const previous = root.getAttribute("data-safe-emulate");
    if (safeAreaEmulation) root.setAttribute("data-safe-emulate", safeAreaEmulation);
    else root.removeAttribute("data-safe-emulate");
    return () => {
      if (previous === null) root.removeAttribute("data-safe-emulate");
      else root.setAttribute("data-safe-emulate", previous);
    };
  }, [stampDocument, safeAreaEmulation]);

  const value = useMemo<OrientationState>(
    () => ({ orientation: resolved, viewport, declared, declare }),
    [resolved, viewport, declared, declare],
  );

  return (
    <OrientationContext.Provider value={value}>
      <div
        className={className ? `mor ${className}` : "mor"}
        data-orientation={resolved}
        {...(safeAreaEmulation ? { "data-safe-emulate": safeAreaEmulation } : null)}
      >
        {children}
      </div>
    </OrientationContext.Provider>
  );
}

export function useOrientationState(): OrientationState {
  const provided = useContext(OrientationContext);
  const viewport = useViewportOrientation();
  const detached = useMemo<OrientationState>(
    () => ({
      orientation: viewport,
      viewport,
      declared: null,
      declare: () => () => {},
    }),
    [viewport],
  );
  return provided ?? detached;
}

export function useOrientation(): Orientation {
  return useOrientationState().orientation;
}

export function useDeclareOrientation(orientation: Orientation | null, active = true): void {
  const { declare } = useOrientationState();
  useEffect(() => {
    if (!active || orientation === null) return undefined;
    return declare(orientation);
  }, [active, orientation, declare]);
}

export default OrientationProvider;
