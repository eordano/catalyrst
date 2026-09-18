import { useEffect, useId, useRef, useState, type ReactNode } from "react";
import DappFooter from "./DappFooter";
import type { ChromeNavigate } from "../web/frames/chrome-nav";
import ChromeLink from "../web/frames/ChromeLink";
import "./chromeshell.css";

type ChromeTab<Id extends string = string> = {
  id: Id;
  label: ReactNode;
  href?: string;
  icon?: ReactNode;
};

type ChromeShellProps<Id extends string = string> = {
  className?: string;
  ariaLabel?: string;
  topbar?: ReactNode;
  subnav?: boolean;
  brand?: ReactNode;
  brandHref?: string;
  tabs?: readonly ChromeTab<Id>[];
  active?: NoInfer<Id>;
  onTab?: (id: NoInfer<Id>) => void;
  onNavigate?: ChromeNavigate;
  tabsLabel?: string;
  right?: ReactNode;
  children?: ReactNode;
  footer?: ReactNode;
};

export default function ChromeShell<Id extends string = string>({
  className = "",
  ariaLabel,
  topbar,
  subnav = true,
  brand = null,
  brandHref,
  tabs = [],
  active,
  onTab,
  onNavigate,
  tabsLabel,
  right = null,
  children,
  footer,
}: ChromeShellProps<Id>) {
  const tabsRef = useRef<HTMLElement>(null);
  const menuId = useId();
  const [menuOpen, setMenuOpen] = useState(false);

  useEffect(() => {
    const nav = tabsRef.current;
    if (!nav) return;
    const sync = () => {
      nav.dataset.overflow = nav.scrollWidth > nav.clientWidth + 1 ? "true" : "false";
    };
    sync();
    const activeEl = nav.querySelector<HTMLElement>(".is-active");
    if (activeEl && nav.scrollWidth > nav.clientWidth + 1) {
      const left = activeEl.offsetLeft - (nav.clientWidth - activeEl.offsetWidth) / 2;
      nav.scrollLeft = Math.max(0, left);
    }
    if (typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(sync);
    ro.observe(nav);
    return () => ro.disconnect();
  }, [active, tabs]);

  useEffect(() => {
    setMenuOpen(false);
  }, [active]);

  useEffect(() => {
    if (!menuOpen) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setMenuOpen(false);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [menuOpen]);

  const renderTab = (tab: ChromeTab<Id>, cls: string) =>
    tab.href ? (
      <ChromeLink
        key={tab.id}
        href={tab.href} onNavigate={onNavigate}
        className={cls + (tab.id === active ? " is-active" : "")}
        aria-current={tab.id === active ? "page" : undefined}
        onClick={() => {
          onTab?.(tab.id);
          setMenuOpen(false);

        }}
      >
        {tab.icon ? <span className="cs__tabicon" aria-hidden="true">{tab.icon}</span> : null}
        {tab.label}
      </ChromeLink>
    ) : (
      <button
        key={tab.id}
        type="button"
        className={cls + (tab.id === active ? " is-active" : "")}
        aria-current={tab.id === active ? "page" : undefined}
        onClick={() => {
          onTab?.(tab.id);
          setMenuOpen(false);
        }}
      >
        {tab.icon ? <span className="cs__tabicon" aria-hidden="true">{tab.icon}</span> : null}
        {tab.label}
      </button>
    );

  const activeLabel = tabs.find((t) => t.id === active)?.label;

  return (
    <div className={"cs ui2" + (className ? " " + className : "")} data-label={ariaLabel}>
      <ChromeLink className="cs__skip" href="#cs-main">Skip to content</ChromeLink>
      {topbar}

      {subnav ? (
        <div className="cs__nav">
          {brand ? (
            brandHref ? (
              <ChromeLink className="cs__brand" href={brandHref} onNavigate={onNavigate}>
                {brand}
              </ChromeLink>
            ) : (
              <div className="cs__brand">{brand}</div>
            )
          ) : null}

          <nav className="cs__tabs" aria-label={tabsLabel} ref={tabsRef}>
            {tabs.map((tab) => renderTab(tab, "cs__tab"))}
          </nav>

          {
}
          {tabs.length > 0 ? (
            <button
              type="button"
              className="cs__menubtn"
              aria-expanded={menuOpen}
              aria-controls={menuId}
              aria-label={tabsLabel}
              onClick={() => setMenuOpen((v) => !v)}
            >
              <svg
                className="cs__menuicon"
                viewBox="0 0 16 16"
                width="16"
                height="16"
                aria-hidden="true"
              >
                <path
                  d="M2 4h12M2 8h12M2 12h12"
                  stroke="currentColor"
                  strokeWidth="1.6"
                  strokeLinecap="round"
                  fill="none"
                />
              </svg>
              <span className="cs__menubtnlabel">{activeLabel ?? "Menu"}</span>
            </button>
          ) : null}

          {menuOpen ? (
            <nav className="cs__menu" id={menuId} aria-label={tabsLabel}>
              {tabs.map((tab) => renderTab(tab, "cs__menuitem"))}
            </nav>
          ) : null}

          {right ? <div className="cs__right">{right}</div> : null}
        </div>
      ) : null}

      <main className="cs__body" id="cs-main" tabIndex={0}>
        {children}
        {footer === false ? null : footer ?? <DappFooter />}
      </main>
    </div>
  );
}
