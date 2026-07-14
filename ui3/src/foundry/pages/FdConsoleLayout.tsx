import type { ReactNode } from "react";
import "./fdconsole.css";

export type FdConsoleTab = "bench" | "trajectories" | "costs" | "pipelines" | "evidence";

/** The one tab list. The route reads it too, so the rail and the URL cannot
 *  disagree. Evidence is run-scoped: it has no collection route, so it appears
 *  only while a reader is on one. */
export const CONSOLE_TABS: readonly {
  id: FdConsoleTab;
  label: string;
  href: string | null;
}[] = [
  { id: "bench", label: "Runs", href: "/foundry/console/bench" },
  { id: "trajectories", label: "Run logs", href: "/foundry/console/trajectories" },
  { id: "costs", label: "Costs", href: "/foundry/console/costs" },
  { id: "pipelines", label: "Pipelines", href: "/foundry/console/pipelines" },
  { id: "evidence", label: "Evidence", href: null },
];

export type FdConsoleLayoutProps = {
  active: FdConsoleTab | null;
  children?: ReactNode;
};

export default function FdConsoleLayout({ active, children }: FdConsoleLayoutProps) {
  return (
    <div className="fd-page fd-console">
      <nav className="fd-console__rail" aria-label="Console">
        <ul className="fd-console__list">
          {CONSOLE_TABS.filter((item) => item.href !== null || item.id === active).map(
            (item) => (
              <li key={item.id}>
                {item.href ? (
                  <a
                    className={
                      "fd-console__item" + (item.id === active ? " is-active" : "")
                    }
                    href={item.href}
                    aria-current={item.id === active ? "page" : undefined}
                  >
                    {item.label}
                  </a>
                ) : (
                  <span className="fd-console__item is-active" aria-current="page">
                    {item.label}
                  </span>
                )}
              </li>
            ),
          )}
        </ul>
      </nav>

      <div className="fd-console__main">{children}</div>
    </div>
  );
}
