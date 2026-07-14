import type { ReactNode } from "react";
import "./fdconsole.css";

export type FdConsoleTab = "bench" | "trajectories" | "costs";

const RAIL: readonly { id: FdConsoleTab; label: string; href: string }[] = [
  { id: "bench", label: "Bench", href: "/foundry/console/bench" },
  {
    id: "trajectories",
    label: "Trajectories",
    href: "/foundry/console/trajectories",
  },
  { id: "costs", label: "Costs", href: "/foundry/console/costs" },
];

export type FdConsoleLayoutProps = {
  active: FdConsoleTab;
  children?: ReactNode;
};

export default function FdConsoleLayout({ active, children }: FdConsoleLayoutProps) {
  return (
    <div className="fd-page fd-console">
      <nav className="fd-console__rail" aria-label="Console">
        <a className="fd-console__cross" href="/foundry/play">
          The games →
        </a>
        <ul className="fd-console__list">
          {RAIL.map((item) => (
            <li key={item.id}>
              <a
                className={
                  "fd-console__item" + (item.id === active ? " is-active" : "")
                }
                href={item.href}
                aria-current={item.id === active ? "page" : undefined}
              >
                {item.label}
              </a>
            </li>
          ))}
        </ul>
      </nav>

      <div className="fd-console__main">{children}</div>
    </div>
  );
}
