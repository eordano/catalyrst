import type { KeyboardEvent, ReactNode } from "react";
import { useRef } from "react";
import "./accountsettingsrail.css";

type TabId = "wallets" | "notifications" | "credits" | "delete";
type Tab = { id: TabId; label: string; danger?: boolean };

const TABS: Tab[] = [
  { id: "wallets", label: "Wallets" },
  { id: "notifications", label: "Email Notifications" },
  { id: "credits", label: "Credits Settings" },
  { id: "delete", label: "Delete Account", danger: true },
];

const ICONS: Record<TabId, ReactNode> = {
  wallets: (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <path d="M3 7a2 2 0 0 1 2-2h12.5A1.5 1.5 0 0 1 19 6.5V8" />
      <path d="M3 7v10a2 2 0 0 0 2 2h14a1 1 0 0 0 1-1V9a1 1 0 0 0-1-1H4" />
      <circle cx="16.5" cy="13.5" r="1.4" />
    </svg>
  ),
  notifications: (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <rect x="3" y="5" width="18" height="14" rx="2" />
      <path d="m3.5 6.5 8.5 6 8.5-6" />
    </svg>
  ),
  credits: (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <rect x="3" y="8" width="18" height="12" rx="2" />
      <path d="M3 12h18M12 8V6a2 2 0 0 0-2-2 2 2 0 0 0 0 4M12 8V6a2 2 0 0 1 2-2 2 2 0 0 1 0 4" />
    </svg>
  ),
  delete: (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <path d="M3 6h18M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2m2 0v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6" />
      <path d="M10 11v6M14 11v6" />
    </svg>
  ),
};

type AccountSettingsRailProps = {
  active?: TabId;
  onTab?: (id: TabId) => void;
};

export default function AccountSettingsRail({
  active = "wallets",
  onTab,
}: AccountSettingsRailProps) {
  const refs = useRef<(HTMLButtonElement | null)[]>([]);

  function pick(id: TabId) {
    onTab?.(id);
  }

  function onKey(e: KeyboardEvent<HTMLButtonElement>, i: number) {
    let next = -1;
    if (e.key === "ArrowRight" || e.key === "ArrowDown") next = (i + 1) % TABS.length;
    else if (e.key === "ArrowLeft" || e.key === "ArrowUp") next = (i - 1 + TABS.length) % TABS.length;
    else if (e.key === "Home") next = 0;
    else if (e.key === "End") next = TABS.length - 1;
    else return;
    e.preventDefault();
    const nextTab = TABS[next];
    if (!nextTab) return;
    pick(nextTab.id);
    refs.current[next]?.focus();
  }

  return (
    <nav className="asr" role="tablist" aria-label="Account sections">
      {TABS.map((t, i) => (
        <button
          key={t.id}
          type="button"
          role="tab"
          aria-selected={t.id === active}
          tabIndex={t.id === active ? 0 : -1}
          ref={(el) => { refs.current[i] = el; }}
          className={"asr__tab" + (t.id === active ? " is-active" : "") + (t.danger ? " is-danger" : "")}
          onClick={() => pick(t.id)}
          onKeyDown={(e) => onKey(e, i)}
        >
          <span className="asr__icon">{ICONS[t.id]}</span>
          {t.label}
        </button>
      ))}
    </nav>
  );
}
