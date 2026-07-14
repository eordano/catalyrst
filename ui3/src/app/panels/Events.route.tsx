import type { QueryClient } from "@tanstack/react-query";
import type { CSSProperties, MouseEvent as ReactMouseEvent, ReactNode } from "react";
import { useCallback, useMemo, useState } from "react";
import { useNavigate } from "react-router";

import EventDetail from "../../explorer/pages/EventDetail";
import JumpLoading, { useJump } from "../../explorer/components/JumpLoading";
import WorldVisitModal from "../../components/WorldVisitModal";
import { sendBridge, getBridge, useBridgeState } from "../../overlay/bridge";
import Dropdown from "../../components/Dropdown";
import "../../explorer/pages/events.css";

import { useEvents, useEventCategories, useEventAttendance } from "../../data/hooks/useEvents";
import { fetchEvents, fetchEventCategories } from "../../data/catalyst/events";
import {
  hueFor,
  eventStart,
  eventXY,
  eventCoords,
  formatEventTime,
  formatEventWhen,
} from "../../data/catalyst/events";
import type { DclEvent as EventItem } from "../../data/catalyst/events";
import { warmSceneAtParcel } from "../../data/catalyst/prewarm";
import { qk, STALE } from "../../data/queryKeys";

type VarStyle = CSSProperties & { [k: `--${string}`]: string | number };
const cssVars = (s: VarStyle): CSSProperties => s;

type DayColumn = { key: string; label: string; today: boolean; items: EventItem[] };

const DEFAULT_PARAMS = { limit: 100 };
const DAY_MS = 86400000;
const DAY_COUNT = 5;

export function prefetch(queryClient: QueryClient) {
  try {
    queryClient.prefetchQuery({
      queryKey: qk.events(DEFAULT_PARAMS),
      queryFn: ({ signal }) => fetchEvents(DEFAULT_PARAMS, { signal }),
      staleTime: STALE.events,
    });
    queryClient.prefetchQuery({
      queryKey: qk.eventCategories(),
      queryFn: ({ signal }) => fetchEventCategories({ signal }),
      staleTime: STALE.eventCategories,
    });
  } catch {
  }
}

function startOfUtcDay(t: number): number {
  const d = new Date(t);
  return Date.UTC(d.getUTCFullYear(), d.getUTCMonth(), d.getUTCDate());
}

function dayLabel(day0: number): string {
  const t0 = startOfUtcDay(Date.now());
  if (day0 === t0) return "Today";
  if (day0 === t0 + DAY_MS) return "Tomorrow";
  return new Date(day0).toLocaleDateString("en-US", {
    weekday: "short",
    month: "short",
    day: "numeric",
    timeZone: "UTC",
  });
}

function formatClock(iso: string | null | undefined): string {
  if (!iso) return "Soon";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "Soon";
  return d
    .toLocaleTimeString("en-US", {
      hour: "numeric",
      minute: "2-digit",
      timeZone: "UTC",
      hour12: true,
    })
    .replace(" ", "");
}

function hostOf(e: EventItem): string {
  return e?.user_name ?? e?.scene_name ?? e?.estate_name ?? "Decentraland";
}

function buildDayColumns(events: EventItem[], dayShift = 0): DayColumn[] {
  const base0 = startOfUtcDay(Date.now());
  const today0 = base0 + dayShift * DAY_MS;
  const cols: DayColumn[] = [];
  for (let i = 0; i < DAY_COUNT; i++) {
    const day0 = today0 + i * DAY_MS;
    cols.push({ key: String(day0), label: dayLabel(day0), today: day0 === base0, items: [] });
  }
  const windowEnd = today0 + DAY_COUNT * DAY_MS;
  for (const e of events) {
    const iso = eventStart(e);
    const t = iso ? new Date(iso).getTime() : NaN;
    if (Number.isNaN(t) || t < today0 || t >= windowEnd) {
      if (e?.live) cols[0]?.items.push(e);
      continue;
    }
    const idx = Math.floor((startOfUtcDay(t) - today0) / DAY_MS);
    (cols[idx] ?? cols[0])?.items.push(e);
  }
  for (const c of cols) {
    c.items.sort((a, b) => {
      const ta = new Date(eventStart(a) ?? 0).getTime() || 0;
      const tb = new Date(eventStart(b) ?? 0).getTime() || 0;
      return ta - tb;
    });
  }
  return cols;
}

function pickFeatured(events: EventItem[]): EventItem | null {
  return (
    events.find((e) => e.highlighted) ||
    events.find((e) => e.trending) ||
    events.find((e) => e.live) ||
    events[0] ||
    null
  );
}

function scheduleText(e: EventItem): string {
  const start = formatEventTime(eventStart(e));
  const finish = e?.finish_at ? formatEventTime(e.finish_at) : null;
  return finish ? `${start} \u{2013} ${finish} (UTC)` : `${start} (UTC)`;
}

const PARCEL_SIZE = 16;

function teleportTo(e: EventItem | null, domEvent?: { preventDefault?: () => void }): boolean {
  if (typeof window === "undefined" || !getBridge()) return false;
  const { x, y } = eventXY(e);
  domEvent?.preventDefault?.();
  sendBridge("Teleport", {
    x: x * PARCEL_SIZE + PARCEL_SIZE / 2,
    z: y * PARCEL_SIZE + PARCEL_SIZE / 2,
  });
  return true;
}

const CARD_JUMP_STYLE: CSSProperties = {
  marginTop: 4,
  alignSelf: "flex-start",
  border: 0,
  borderRadius: "var(--r-pill)",
  background: "linear-gradient(90deg, #ff4d8d, #ec2e7a)",
  color: "#fff",
  font: "inherit",
  fontSize: 9.5,
  fontWeight: 800,
  letterSpacing: 0.4,
  padding: "4px 12px",
  cursor: "pointer",
};

type EventCardProps = {
  ev: EventItem;
  featured?: boolean;
  onOpen: (ev: EventItem) => void;
  onJump: (ev: EventItem, e: ReactMouseEvent) => boolean;
};

function EventCard({ ev, featured, onOpen, onJump }: EventCardProps) {
  const live = !!ev.live;
  const badge = live ? `${ev.total_attendees} LIVE` : ev.trending ? "TRENDING" : null;
  const thumbStyle: VarStyle = { "--hue": hueFor(ev.id) };
  if (ev.image) {
    thumbStyle.backgroundImage = `linear-gradient(180deg, rgba(0,0,0,0) 55%, rgba(0,0,0,.55)), url("${ev.image}")`;
    thumbStyle.backgroundSize = "cover";
    thumbStyle.backgroundPosition = "center";
  }
  return (
    <div
      className={"ev__card" + (featured ? " is-featured" : "")}
      role="button"
      tabIndex={0}
      data-sb-linkto="Explorer/Pages/EventDetail"
      onClick={() => onOpen(ev)}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onOpen(ev);
        }
      }}
    >
      <div className="ev__thumb" style={thumbStyle} aria-hidden="true">
        {badge ? <span className={"ev__badge" + (live ? " is-live" : "")}>{badge}</span> : null}
      </div>
      <div className="ev__cardbody">
        <h3 className="ev__cardtitle u-truncate">{ev.name || "Untitled event"}</h3>
        <p className="ev__cardorg u-truncate">By {hostOf(ev)}</p>
        <p className="ev__cardtime">
          <span className="ev__clock" aria-hidden="true">&#x1F551;</span>
          {formatClock(eventStart(ev))}
        </p>
        <button
          type="button"
          style={CARD_JUMP_STYLE}
          data-sb-linkto="Explorer/Workflows/SceneLoading"
          aria-label={`Jump in to ${ev.name || "event"}`}
          onClick={(e) => {
            e.stopPropagation();
            if (!onJump(ev, e)) onOpen(ev);
          }}
          onKeyDown={(e) => e.stopPropagation()}
        >
          jump in
        </button>
      </div>
    </div>
  );
}

function SkeletonGrid() {
  const cols = Array.from({ length: DAY_COUNT });
  return (
    <div className="ev__grid" aria-busy="true">
      {cols.map((_, ci) => (
        <div key={ci} className="ev__gridcol">
          {Array.from({ length: 3 }).map((__, ri) => (
            <div key={ri} className="ev__card" style={{ opacity: 0.5 }}>
              <div className="ev__thumb" style={cssVars({ "--hue": (ci * 53 + ri * 31) % 360 })} />
              <div className="ev__cardbody">
                <h3 className="ev__cardtitle u-truncate">Loading&#x2026;</h3>
                <p className="ev__cardorg u-truncate">&nbsp;</p>
              </div>
            </div>
          ))}
        </div>
      ))}
    </div>
  );
}

function Notice({ children }: { children: ReactNode }) {
  return (
    <div
      className="ev__grid"
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        minHeight: 200,
        color: "rgba(255,255,255,0.8)",
        textAlign: "center",
        gap: 12,
      }}
    >
      <div>{children}</div>
    </div>
  );
}

export default function EventsPanel() {
  const navigate = useNavigate();
  const [category, setCategory] = useState("");
  const [selected, setSelected] = useState<EventItem | null>(null);
  const [confirmWorld, setConfirmWorld] = useState<{ realm: string; title?: string } | null>(null);

  const { jumping, stalled, beginJump, cancelJump, confirmJump } = useJump(() => navigate("/"));

  const jumpTo = useCallback(
    (ev: EventItem | null, domEvent?: { preventDefault?: () => void }) => {
      if (typeof window === "undefined" || !getBridge()) return false;
      if (ev?.world) {
        if (!ev.server) return false;
        domEvent?.preventDefault?.();
        setConfirmWorld({ realm: ev.server, title: ev.name ?? undefined });
        return true;
      }
      if (!teleportTo(ev, domEvent)) return false;
      const { x, y } = eventXY(ev);
      warmSceneAtParcel(x, y);
      beginJump(ev?.name || "event");
      return true;
    },
    [beginJump],
  );

  const confirmVisitWorld = useCallback(() => {
    if (!confirmWorld) return;
    setConfirmWorld(null);
    sendBridge("ChangeRealm", { realm: confirmWorld.realm });
    beginJump(confirmWorld.title || confirmWorld.realm);
  }, [confirmWorld, beginJump]);

  const params = useMemo(
    () => (category ? { ...DEFAULT_PARAMS, category } : DEFAULT_PARAMS),
    [category],
  );

  const evq = useEvents(params);
  const catq = useEventCategories();

  const signerAddress = useBridgeState((s) => s.identity.address);
  const attendance = useEventAttendance(selected?.id ?? null, signerAddress);
  // Interest is hidden on live events (unity-explorer parity) and needs a
  // signer behind the bridge; either gate missing keeps the ICS reminder.
  const canToggleInterest = !!selected && !selected.live && !!signerAddress && !!getBridge();

  const events: EventItem[] = evq.data?.data ?? [];
  const featured = useMemo(() => pickFeatured(events), [events]);
  const [dayShift, setDayShift] = useState(0);
  const columns = useMemo(
    () => buildDayColumns(featured ? events.filter((e) => e.id !== featured.id) : events, dayShift),
    [events, featured, dayShift],
  );
  const categories = catq.data ?? [];
  const ALL_CATEGORIES_LABEL = "All categories";
  const categoryLabels = [ALL_CATEGORIES_LABEL, ...categories.map((c) => c.i18n?.en || c.name)];
  const categoryLabelFor = (value: string) =>
    value ? categories.find((c) => c.name === value)?.i18n?.en || value : ALL_CATEGORIES_LABEL;
  const categoryValueFor = (label: string) =>
    label === ALL_CATEGORIES_LABEL ? "" : categories.find((c) => (c.i18n?.en || c.name) === label)?.name ?? "";

  const onOpen = (ev: EventItem) => setSelected(ev);
  const onClose = () => setSelected(null);

  const detail = selected
    ? {
        id: selected.id,
        title: selected.name ?? "Untitled event",
        when: formatEventWhen(eventStart(selected)),
        host: hostOf(selected),
        description: selected.description ?? "No description provided.",
        schedule: scheduleText(selected),
        location: eventCoords(selected),
        image: selected.image_vertical ?? selected.image ?? undefined,
        start: eventStart(selected) ?? undefined,
        finish: selected.finish_at ?? undefined,
        url: selected.url ?? undefined,
      }
    : null;

  const showSkeleton = evq.isLoading && events.length === 0;
  const showError = evq.isError && events.length === 0;
  const showEmpty = !evq.isLoading && !evq.isError && events.length === 0;

  const featuredHero: CSSProperties | undefined =
    featured && (featured.image_vertical || featured.image)
      ? {
          backgroundImage: `url("${featured.image_vertical || featured.image}")`,
          backgroundSize: "cover",
          backgroundPosition: "center",
        }
      : undefined;

  return (
    <div className="ev">
      <div className="ev__head">
        <h1 className="ev__title">Events</h1>
        <div className="ev__headactions">
          <div style={{ width: 200 }}>
            <Dropdown
              ariaLabel="Filter events by category"
              options={categoryLabels}
              value={categoryLabelFor(category)}
              onChange={(label) => setCategory(categoryValueFor(label))}
            />
          </div>
          <button type="button" className="ev__create" disabled title="Sign-in required (read-only milestone)">
            + CREATE EVENT
          </button>
        </div>
      </div>

      <div className="ev__body">
        <div className="ev__main">
          <div className="ev__carousel">
            <button type="button" className="ev__chev" aria-label="Previous days" onClick={() => setDayShift((d) => Math.max(0, d - DAY_COUNT))}>&#x2039;</button>
            <div className="ev__cols" role="tablist" aria-label="Event days">
              {columns.map((c) => (
                <div key={c.key} className="ev__col">
                  <span className={"ev__daylabel" + (c.today ? " is-today" : "")}>{c.label}</span>
                </div>
              ))}
            </div>
            <button type="button" className="ev__chev" aria-label="Next days" onClick={() => setDayShift((d) => d + DAY_COUNT)}>&#x203A;</button>
          </div>

          {showSkeleton ? (
            <SkeletonGrid />
          ) : showError ? (
            <Notice>
              <p style={{ margin: "0 0 12px" }}>Couldn&#x2019;t load events right now.</p>
              <button type="button" className="ev__todaybtn" onClick={() => evq.refetch()}>
                Retry
              </button>
            </Notice>
          ) : showEmpty ? (
            <Notice>
              <p style={{ margin: 0 }}>
                No events scheduled{category ? " in this category" : ""} for the next few days.
              </p>
            </Notice>
          ) : (
            <div className="ev__grid">
              {columns.map((c) => (
                <div key={c.key} className="ev__gridcol">
                  {c.items.map((ev) => (
                    <EventCard key={ev.id} ev={ev} onOpen={onOpen} onJump={jumpTo} />
                  ))}
                </div>
              ))}
            </div>
          )}
        </div>

        {featured ? (
          <aside className="ev__feature" aria-label="Featured event">
            <div
              className="ev__featurehero"
              style={featuredHero}
              role="button"
              tabIndex={0}
              onClick={() => onOpen(featured)}
              onKeyDown={(e) => {
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault();
                  onOpen(featured);
                }
              }}
            >
              {!featuredHero ? (
                <span className="ev__featuretag">{featured.name || "Featured"}</span>
              ) : null}
            </div>
            <div className="ev__featurebody">
              <p className="ev__featuredate">
                <span className="ev__clock" aria-hidden="true">&#x1F4C5;</span>
                {formatEventWhen(eventStart(featured))}
              </p>
              <h2 className="ev__featuretitle">{featured.name || "Untitled event"}</h2>
              <p className="ev__featureorg">Organized by {hostOf(featured)}</p>
              <div className="ev__featureactions">
                <button
                  type="button"
                  className="ev__remind"
                  onClick={(e) => {
                    if (!jumpTo(featured, e)) onOpen(featured);
                  }}
                >
                  jump in
                </button>
                <button type="button" className="ev__iconbtn" aria-label="Details" onClick={() => onOpen(featured)}>
                  &#x2197;
                </button>
              </div>
            </div>
          </aside>
        ) : null}
      </div>

      {detail ? (
        <EventDetail
          event={detail}
          jumpHref={selected?.url ?? undefined}
          onJumpIn={() => {
            if (jumpTo(selected)) onClose();
          }}
          onClose={onClose}
          interested={attendance.attending}
          interestedCount={attendance.count ?? selected?.total_attendees}
          interestedError={attendance.error}
          interestedPending={attendance.pending}
          onToggleInterested={canToggleInterest ? attendance.toggle : undefined}
        />
      ) : null}
      {confirmWorld && (
        <WorldVisitModal
          worldName={confirmWorld.realm}
          title={confirmWorld.title}
          onCancel={() => setConfirmWorld(null)}
          onConfirm={confirmVisitWorld}
        />
      )}
      {jumping && (
        <JumpLoading
          name={jumping}
          stalled={stalled}
          onCancel={cancelJump}
          onEnterAnyway={confirmJump}
        />
      )}
    </div>
  );
}
