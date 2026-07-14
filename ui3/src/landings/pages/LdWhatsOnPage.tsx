import type { ComponentProps, MouseEvent } from "react";

import StWhatSOn from "../../web/pages/StWhatSOn";
import "./ldwhatsonpage.css";

type StWhatSOnProps = ComponentProps<typeof StWhatSOn>;

export type LdWhatsOnFilter = { id: string; label: string };

export type LdWhatsOnEventLink = {
  id: string;
  href: string;
  name: string;
  live?: boolean;
};

type LdWhatsOnPageProps = {
  liveNow?: StWhatSOnProps["liveNow"];
  upcoming?: StWhatSOnProps["upcoming"];
  allDays?: StWhatSOnProps["allDays"];
  dayLabels?: string[];
  filters?: LdWhatsOnFilter[];
  activeFilter?: string;
  onFilterSelect?: (id: string) => void;
  eventLinks?: LdWhatsOnEventLink[];
  onEventLinkClick?: (
    link: LdWhatsOnEventLink,
    e: MouseEvent<HTMLAnchorElement>,
  ) => void;
};

export default function LdWhatsOnPage({
  liveNow = [],
  upcoming = [],
  allDays = [],
  dayLabels = [],
  filters = [],
  activeFilter = "",
  onFilterSelect = undefined,
  eventLinks = [],
  onEventLinkClick = undefined,
}: LdWhatsOnPageProps) {
  return (
    <div className="whatson-route">
      <StWhatSOn
        liveNow={liveNow}
        upcoming={upcoming}
        allDays={allDays}
        dayLabels={dayLabels}
        filterBar={
          <div className="whatson-route__cats" role="tablist" aria-label="Event filters">
            {filters.map((f) => (
              <CategoryPill
                key={f.id || "all"}
                label={f.label}
                active={activeFilter === f.id}
                onSelect={() => onFilterSelect?.(f.id)}
              />
            ))}
          </div>
        }
      />

      {eventLinks.length > 0 && (
        <nav className="whatson-route__links" aria-label="All events">
          {eventLinks.map((link) => (
            <a
              key={link.id}
              href={link.href}
              onClick={(e) => onEventLinkClick?.(link, e)}
              className="whatson-route__link"
            >
              {link.name}
              {link.live ? " — LIVE" : ""}
            </a>
          ))}
        </nav>
      )}

      {eventLinks.length === 0 && (
        <p className="whatson-route__empty">
          No events right now. Check back soon.
        </p>
      )}
    </div>
  );
}

type CategoryPillProps = { label: string; active: boolean; onSelect: () => void };

function CategoryPill({ label, active, onSelect }: CategoryPillProps) {
  return (
    <button
      type="button"
      role="tab"
      aria-selected={active}
      className={"whatson-route__pill" + (active ? " is-active" : "")}
      onClick={onSelect}
    >
      {label}
    </button>
  );
}
