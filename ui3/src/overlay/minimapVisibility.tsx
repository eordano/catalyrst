import type { ReactNode } from "react";
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
} from "react";

type MinimapVisibility = {
  minimapHidden: boolean;
  userHidden: boolean;
  requestHide: () => () => void;
  toggleUserHidden: () => void;
};

const MinimapVisibilityContext = createContext<MinimapVisibility | null>(null);

const STORAGE_KEY = "dcl.minimap.userHidden";

const INERT: MinimapVisibility = {
  minimapHidden: false,
  userHidden: false,
  requestHide: () => () => {},
  toggleUserHidden: () => {},
};

function readUserHidden(initiallyVisible: boolean): boolean {
  try {
    const saved = localStorage.getItem(STORAGE_KEY);
    return saved === null ? !initiallyVisible : saved !== "0";
  } catch {
    return true;
  }
}

export function MinimapVisibilityProvider({ children, initiallyVisible = false }: { children: ReactNode; initiallyVisible?: boolean }) {
  const [hideCount, setHideCount] = useState(0);
  const [userHidden, setUserHidden] = useState(() => readUserHidden(initiallyVisible));

  const requestHide = useCallback((): (() => void) => {
    setHideCount((n) => n + 1);
    let released = false;
    return () => {
      if (released) return;
      released = true;
      setHideCount((n) => Math.max(0, n - 1));
    };
  }, []);

  const toggleUserHidden = useCallback(() => {
    setUserHidden((h) => {
      const next = !h;
      try {
        localStorage.setItem(STORAGE_KEY, next ? "1" : "0");
      } catch {
      }
      return next;
    });
  }, []);

  const value = useMemo<MinimapVisibility>(
    () => ({ minimapHidden: hideCount > 0, userHidden, requestHide, toggleUserHidden }),
    [hideCount, userHidden, requestHide, toggleUserHidden],
  );

  return (
    <MinimapVisibilityContext.Provider value={value}>
      {children}
    </MinimapVisibilityContext.Provider>
  );
}

export function useMinimapVisibility(): MinimapVisibility {
  return useContext(MinimapVisibilityContext) ?? INERT;
}

export function useHideMinimapWhileMounted(active = true): void {
  const { requestHide } = useMinimapVisibility();
  useEffect(() => {
    if (!active) return undefined;
    return requestHide();
  }, [active, requestHide]);
}
