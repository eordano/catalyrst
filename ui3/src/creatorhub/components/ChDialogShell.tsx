import { useId } from "react";
import type { KeyboardEvent, ReactNode } from "react";
import Modal from "../../components/Modal";
import { Close } from "../../atoms/icons";
import "./chdialogshell.css";

export type ChDialogShellTab = { value: string; label: string };
export type ChDialogShellVariant = "modal" | "panel";

type ChDialogShellProps = {
  variant?: ChDialogShellVariant;
  icon?: ReactNode;
  title: ReactNode;
  ariaLabel: string;
  onClose?: () => void;
  closeOnBackdrop?: boolean;
  tabs?: ChDialogShellTab[];
  activeTab?: string;
  onTabChange?: (value: string) => void;
  hideTabs?: boolean;
  width?: number | string;
  className?: string;
  children?: ReactNode;
};

export function onTablistKeyDown(e: KeyboardEvent<HTMLElement>) {
  if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(e.key)) return;
  const tabs = [...e.currentTarget.querySelectorAll<HTMLElement>('[role="tab"]')];
  if (!tabs.length) return;
  e.preventDefault();
  const idx = tabs.indexOf(document.activeElement as HTMLElement);
  const next =
    e.key === "Home" ? 0 :
    e.key === "End" ? tabs.length - 1 :
    e.key === "ArrowDown" ? (idx + 1) % tabs.length :
    (idx - 1 + tabs.length) % tabs.length;
  tabs[next]?.focus();
  tabs[next]?.click();
}

export default function ChDialogShell({
  variant = "modal",
  icon,
  title,
  ariaLabel,
  onClose,
  closeOnBackdrop = false,
  tabs,
  activeTab,
  onTabChange,
  hideTabs = false,
  width = 625,
  className = "",
  children,
}: ChDialogShellProps) {
  const titleId = useId();

  const chrome = (
    <>
      <header className="chdlg__header">
        {icon ? <span className="chdlg__headericon">{icon}</span> : null}
        <h6 className="chdlg__title" id={titleId}>{title}</h6>
      </header>

      <div className="chdlg__layout">
        {tabs && !hideTabs ? (
          <nav
            className="chdlg__tabs"
            role="tablist"
            aria-orientation="vertical"
            onKeyDown={onTablistKeyDown}
          >
            {tabs.map((tab) => (
              <button
                key={tab.value}
                type="button"
                role="tab"
                aria-selected={tab.value === activeTab}
                className={"chdlg__tab" + (tab.value === activeTab ? " is-selected" : "")}
                onClick={onTabChange ? () => onTabChange(tab.value) : undefined}
              >
                {tab.label}
              </button>
            ))}
          </nav>
        ) : null}
        <div className="chdlg__content">{children}</div>
      </div>
    </>
  );

  if (variant === "panel") {
    return (
      <section
        className={"chdlg chdlg--panel" + (className ? " " + className : "")}
        style={{ width }}
      >
        {onClose ? (
          <button
            type="button"
            className="chdlg__close"
            aria-label="Close"
            onClick={onClose}
          >
            <Close />
          </button>
        ) : null}
        {chrome}
      </section>
    );
  }

  return (
    <Modal
      width={width}
      className={"chdlg" + (className ? " " + className : "")}
      ariaLabel={ariaLabel}
      onClose={onClose}
      closeOnBackdrop={closeOnBackdrop}
    >
      {chrome}
    </Modal>
  );
}
