import type { ReactNode } from "react";
import { useEffect, useRef, useState } from "react";
import type { ChromeNavigate } from "./chrome-nav";
import ChromeLink from "./ChromeLink";
import "./accountmenu.css";

type AccountMenuLink = { label: string; href: string };

type AccountMenuProps = {
  name?: string;
  account?: string;
  links?: AccountMenuLink[];
  onSignOut?: () => void;
  onSwitchAccount?: () => void;
  onNavigate?: ChromeNavigate;
  triggerClassName?: string;
  triggerLabel?: string;
  align?: "left" | "right";
  placement?: "below" | "above";
  className?: string;
  children: ReactNode;
};

function shortAddress(addr: string): string {
  return addr && addr.length > 12 ? `${addr.slice(0, 6)}\u{2026}${addr.slice(-4)}` : addr;
}

export default function AccountMenu({
  name = "",
  account = "",
  links = [],
  onSignOut = undefined,
  onSwitchAccount = undefined,
  onNavigate = undefined,
  triggerClassName = "",
  triggerLabel = "My account",
  align = "right",
  placement = "below",
  className = "",
  children,
}: AccountMenuProps) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  const close = () => setOpen(false);
  const run = (fn: () => void) => () => {
    close();
    fn();
  };

  return (
    <div className={"acm" + (className ? " " + className : "")} ref={rootRef}>
      <button
        type="button"
        className={triggerClassName}
        aria-label={triggerLabel}
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
      >
        {children}
      </button>
      {open ? (
        <div
          className={`acm__menu acm__menu--${align} acm__menu--${placement}`}
          role="menu"
          aria-label={triggerLabel}
        >
          <div className="acm__head">
            <span className="acm__name">{name || "My Account"}</span>
            {account ? (
              <span className="acm__addr" title={account}>{shortAddress(account)}</span>
            ) : null}
          </div>
          {links.map((l) => (
            <ChromeLink
              key={l.href + l.label}
              className="acm__item"
              role="menuitem"
              href={l.href} onNavigate={onNavigate}
              onClick={() => {
                close();

              }}
            >
              {l.label}
            </ChromeLink>
          ))}
          {onSwitchAccount ? (
            <button type="button" className="acm__item" role="menuitem" onClick={run(onSwitchAccount)}>
              Switch account
            </button>
          ) : null}
          {onSignOut ? (
            <button
              type="button"
              className="acm__item acm__item--signout"
              role="menuitem"
              onClick={run(onSignOut)}
            >
              Log out
            </button>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
