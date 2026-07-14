import type { ReactNode } from "react";
import DappFooter from "./DappFooter";
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
  tabs?: readonly ChromeTab<Id>[];
  active?: NoInfer<Id>;
  onTab?: (id: NoInfer<Id>) => void;
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
  tabs = [],
  active,
  onTab,
  tabsLabel,
  right = null,
  children,
  footer,
}: ChromeShellProps<Id>) {
  return (
    <div className={"cs ui2" + (className ? " " + className : "")} data-label={ariaLabel}>
      <a className="cs__skip" href="#cs-main">Skip to content</a>
      {topbar}

      {subnav ? (
        <div className="cs__nav">
          {brand ? <div className="cs__brand">{brand}</div> : null}

          <nav className="cs__tabs" aria-label={tabsLabel}>
            {tabs.map((tab) =>
              tab.href ? (
                <a
                  key={tab.id}
                  href={tab.href}
                  className={"cs__tab" + (tab.id === active ? " is-active" : "")}
                  aria-current={tab.id === active ? "page" : undefined}
                  onClick={() => onTab?.(tab.id)}
                >
                  {tab.icon ? <span className="cs__tabicon" aria-hidden="true">{tab.icon}</span> : null}
                  {tab.label}
                </a>
              ) : (
                <button
                  key={tab.id}
                  type="button"
                  className={"cs__tab" + (tab.id === active ? " is-active" : "")}
                  aria-current={tab.id === active ? "page" : undefined}
                  onClick={() => onTab?.(tab.id)}
                >
                  {tab.icon ? <span className="cs__tabicon" aria-hidden="true">{tab.icon}</span> : null}
                  {tab.label}
                </button>
              )
            )}
          </nav>

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
