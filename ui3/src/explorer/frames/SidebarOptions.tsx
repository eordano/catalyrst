import { useEffect, useRef, useState } from "react";
import ContextMenu from "../../components/ContextMenu";

function saved(key: string): boolean {
  try { return localStorage.getItem(key) === "1"; } catch { return false; }
}

export function useSidebarPreferences() {
  const [large, setLarge] = useState(() => saved("dcl.sidebar.large"));
  useEffect(() => {
    const root = document.documentElement;
    root.style.setProperty("--sidebar-scale", large ? "1.5" : "1");
    root.style.setProperty("--sidebar-width", large ? "69px" : "46px");
    try {
      localStorage.setItem("dcl.sidebar.large", large ? "1" : "0");
      localStorage.removeItem("dcl.sidebar.autoHide");
    } catch {}
    return () => {
      root.style.removeProperty("--sidebar-scale");
      root.style.removeProperty("--sidebar-width");
    };
  }, [large]);
  return { large, setLarge };
}

export default function SidebarOptions({ large, setLarge, onClose }: ReturnType<typeof useSidebarPreferences> & { onClose: (restoreFocus?: boolean) => void }) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const dismiss = (e: PointerEvent) => {
      if (e.target instanceof Element && !ref.current?.contains(e.target) && !e.target.closest(".sb__cfg")) onClose(false);
    };
    document.addEventListener("pointerdown", dismiss, true);
    return () => document.removeEventListener("pointerdown", dismiss, true);
  }, [onClose]);
  return (
    <div ref={ref} className="sb__options">
      <ContextMenu autoFocus onClose={onClose} items={[
        { kind: "title", label: "Sidebar" },
        { kind: "toggle", label: "Larger sidebar (150%)", checked: large, onChange: setLarge },
      ]} />
    </div>
  );
}
