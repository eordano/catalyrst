import type { KeyboardEvent, MouseEventHandler, ReactNode } from "react";
import { useEffect, useRef, useState } from "react";
import { Avatar } from "../atoms/primitives";
import "../atoms/toggle.css";
import "./contextmenu.css";

type ButtonItemFields = {
  label?: ReactNode;
  icon?: ReactNode;
  danger?: boolean;
  disabled?: boolean;
  to?: string;
  onClick?: MouseEventHandler<HTMLButtonElement>;
};

export type ContextMenuItem =
  | { kind: "separator" }
  | { kind: "title"; label: ReactNode }
  | { kind: "caption"; label: ReactNode; avatar?: boolean; hue?: number }
  | { kind: "header"; name?: ReactNode; tag?: ReactNode; address?: string; hue?: number }
  | { kind: "toggle"; label?: ReactNode; icon?: ReactNode; checked?: boolean; onChange?: (next: boolean) => void }
  | ({ kind: "button" | "submenu" } & ButtonItemFields);

type ToggleMenuItem = Extract<ContextMenuItem, { kind: "toggle" }>;

type ContextMenuProps = {
  items?: ContextMenuItem[];
  onClose?: () => void;
  autoFocus?: boolean;
};

function shortAddress(a: string | undefined): string | undefined {
  if (typeof a !== "string" || a.length <= 13) return a;
  return a.slice(0, 6) + "\u{2026}" + a.slice(-4);
}

export default function ContextMenu({ items = [], onClose, autoFocus = false }: ContextMenuProps) {
  const rootRef = useRef<HTMLDivElement>(null);

  const enabledItems = (): HTMLElement[] =>
    rootRef.current
      ? Array.from(
          rootRef.current.querySelectorAll<HTMLElement>(
            '[role="menuitem"]:not(:disabled), [role="menuitemcheckbox"]:not(:disabled)'
          )
        )
      : [];

  useEffect(() => {
    if (!autoFocus) return;
    const els = enabledItems();
    els.forEach((el, i) => { el.tabIndex = i === 0 ? 0 : -1; });
    els[0]?.focus();
  }, [autoFocus]);

  function onKeyDown(e: KeyboardEvent<HTMLDivElement>) {
    if (e.key === "Escape") {
      e.preventDefault();
      onClose?.();
      return;
    }
    const els = enabledItems();
    if (els.length === 0) return;
    const cur = document.activeElement instanceof HTMLElement ? els.indexOf(document.activeElement) : -1;
    let next: number;
    if (e.key === "ArrowDown") next = cur < 0 ? 0 : (cur + 1) % els.length;
    else if (e.key === "ArrowUp") next = cur < 0 ? els.length - 1 : (cur - 1 + els.length) % els.length;
    else if (e.key === "Home") next = 0;
    else if (e.key === "End") next = els.length - 1;
    else return;
    e.preventDefault();
    els.forEach((el, i) => { el.tabIndex = i === next ? 0 : -1; });
    els[next]?.focus();
  }

  return (
    <div className="ctx" role="menu" ref={rootRef} onKeyDown={onKeyDown}>
      {items.map((it, i) => {
        if (it.kind === "separator") return <div className="ctx__sep" role="separator" key={i} />;
        if (it.kind === "title") {
          return <div className="ctx__title" key={i}>{it.label}</div>;
        }
        if (it.kind === "caption") {
          return (
            <div className="ctx__caption" key={i}>
              {it.avatar != null && <Avatar hue={it.hue ?? 280} size={20} />}
              <span className="ctx__captionlabel">{it.label}</span>
            </div>
          );
        }
        if (it.kind === "header") {
          return (
            <div className="ctx__header" key={i}>
              <Avatar hue={it.hue ?? 280} size={42} />
              <div className="ctx__hinfo">
                <div className="ctx__hname u-truncate">{it.name}<span className="ctx__htag">{it.tag}</span></div>
                <span className="u-wallet">{shortAddress(it.address)} &#x29C9;</span>
              </div>
            </div>
          );
        }
        if (it.kind === "toggle") return <ToggleItem item={it} key={i} />;
        return (
          <button
            className={"ctx__item" + (it.danger ? " ctx__item--danger" : "") + (it.disabled ? " ctx__item--disabled" : "")}
            role="menuitem" key={i}
            data-sb-linkto={it.to || undefined}
            onClick={it.onClick}
            disabled={it.disabled}
            aria-disabled={it.disabled || undefined}
          >
            {it.icon && <span className="ctx__icon">{it.icon}</span>}
            <span className="ctx__label">{it.label}</span>
            {it.kind === "submenu" && <span className="ctx__chev">&#x203A;</span>}
          </button>
        );
      })}
    </div>
  );
}

function ToggleItem({ item }: { item: ToggleMenuItem }) {
  const [internal, setInternal] = useState(!!item.checked);
  const isControlled = item.checked !== undefined;
  const on = isControlled ? item.checked : internal;
  function change() {
    const next = !on;
    if (!isControlled) setInternal(next);
    item.onChange && item.onChange(next);
  }
  return (
    <button
      type="button"
      className="ctx__item ctx__item--toggle"
      role="menuitemcheckbox" aria-checked={on}
      onClick={change}
    >
      {item.icon && <span className="ctx__icon">{item.icon}</span>}
      <span className="ctx__label">{item.label}</span>
      <span className={"toggle" + (on ? " is-on" : "")} aria-hidden="true">
        <span className="toggle__knob" />
      </span>
    </button>
  );
}
