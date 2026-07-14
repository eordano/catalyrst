import type { MouseEvent, ReactNode } from "react";

import "./ldeventschedulepage.css";

export type LdScheduleMode = "create" | "edit";

type LdEventSchedulePageProps = {
  mode?: LdScheduleMode;
  onModeClick?: (mode: LdScheduleMode, e: MouseEvent<HTMLAnchorElement>) => void;
  children?: ReactNode;
};

export default function LdEventSchedulePage({
  mode = "create",
  onModeClick = undefined,
  children = null,
}: LdEventSchedulePageProps) {
  return (
    <main className="event-schedule-route">
      <nav className="event-schedule-route__modes" aria-label="Schedule wizard mode">
        <a
          href="?mode=create"
          className="event-schedule-route__modelink"
          aria-current={mode === "create" ? "page" : undefined}
          onClick={(e) => onModeClick?.("create", e)}
        >
          Create schedule
        </a>
        <a
          href="?mode=edit"
          className="event-schedule-route__modelink"
          aria-current={mode === "edit" ? "page" : undefined}
          onClick={(e) => onModeClick?.("edit", e)}
        >
          Edit schedule
        </a>
      </nav>
      {children}
    </main>
  );
}
