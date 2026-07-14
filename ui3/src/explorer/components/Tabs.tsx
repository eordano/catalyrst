import type { KeyboardEvent, ReactNode } from "react";
import { useRef, useState } from "react";
import "./tabs.css";

type Tab = { id: string; label: ReactNode };
type TabsProps = {
  tabs: Tab[];
  active?: string;
  onChange?: (id: string) => void;
  variant?: "pill" | "underline";
};

export default function Tabs({ tabs, active, onChange, variant = "pill" }: TabsProps) {
  const [internal, setInternal] = useState<string | undefined>(active ?? tabs[0]?.id);
  const cur = active ?? internal;
  const refs = useRef<(HTMLButtonElement | null)[]>([]);
  function pick(id: string) { if (active === undefined) setInternal(id); onChange && onChange(id); }
  function onKey(e: KeyboardEvent<HTMLButtonElement>, i: number) {
    if (e.key !== "ArrowRight" && e.key !== "ArrowLeft") return;
    e.preventDefault();
    const d = e.key === "ArrowRight" ? 1 : -1;
    const next = (i + d + tabs.length) % tabs.length;
    const nextTab = tabs[next];
    if (!nextTab) return;
    pick(nextTab.id);
    refs.current[next]?.focus();
  }
  return (
    <div className={"tabs" + (variant === "underline" ? " tabs--underline" : "")} role="tablist">
      {tabs.map((t, i) => (
        <button
          key={t.id}
          role="tab"
          aria-selected={t.id === cur}
          tabIndex={t.id === cur ? 0 : -1}
          ref={(el) => { refs.current[i] = el; }}
          className={"tabs__tab" + (t.id === cur ? " is-active" : "")}
          onClick={() => pick(t.id)}
          onKeyDown={(e) => onKey(e, i)}
        >
          {t.label}
        </button>
      ))}
    </div>
  );
}
