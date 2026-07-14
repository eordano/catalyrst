import { type FormEvent, useState } from "react";

import { plural, stampUTC } from "../fmt";
import Button from "../../atoms/Button";
import Spinner from "../../atoms/Spinner";
import FdPersonaChip, { type FdChipActor } from "../components/FdPersonaChip";
import FdSection, { FdPageHead } from "../components/FdSection";
import FdTime from "../components/FdTime";
import FdStat from "../components/FdStat";
import "../components/fdstat.css";
import "../components/fdpersonachip.css";
import "./fdsessions.css";

export type FdSessionHost = { name: string } | { badge: string };

export type FdSessionOccurrenceVM = {
  seriesId: string;
  title: string;
  body: string;
  sceneId: string | null;
  sceneTitle: string | null;
  cadence: "once" | "weekly";
  occurrenceAt: string;
  durationMinutes: number;
  host: FdSessionHost;
  rsvpCount: number;
  viewerRsvped: boolean;
  label: string;
};

export type FdSessionsCreateValues = {
  title: string;
  body: string;
  sceneId: string;
  cadence: "once" | "weekly";
  firstAt: string;
  durationMinutes: string;
};

export type FdSessionsPageProps = {
  occurrences: readonly FdSessionOccurrenceVM[];
  canHost: boolean;
  createOpen: boolean;
  onToggleCreate: () => void;
  error?: string | null;
  pending?: boolean;
  onRsvp: (values: { seriesId: string; occurrenceAt: string }) => void;
  onWithdraw: (values: { seriesId: string; occurrenceAt: string }) => void;
  onCreate: (values: FdSessionsCreateValues) => void;
  onRetire: (seriesId: string) => void;
};

function durationLabel(minutes: number): string {
  if (minutes < 60) return `${minutes} min`;
  const h = Math.floor(minutes / 60);
  const rem = minutes % 60;
  const hours = `${h} hr${h === 1 ? "" : "s"}`;
  return rem === 0 ? hours : `${hours} ${rem} min`;
}

function hostChip(host: FdSessionHost): FdChipActor {
  return "name" in host ? { name: host.name } : { badge: host.badge };
}

/** Retiring drops every future date of a series, so the second click is the
 *  one that does it. */
function RetireControl({
  pending,
  onRetire,
}: {
  pending: boolean;
  onRetire: () => void;
}) {
  const [armed, setArmed] = useState(false);
  if (!armed) {
    return (
      <Button variant="ghost" size="sm" disabled={pending} onClick={() => setArmed(true)}>
        Retire series
      </Button>
    );
  }
  return (
    <>
      <Button variant="ghost" size="sm" disabled={pending} onClick={onRetire}>
        Retire every future date
      </Button>
      <Button variant="ghost" size="sm" disabled={pending} onClick={() => setArmed(false)}>
        Keep it
      </Button>
    </>
  );
}

export default function FdSessionsPage({
  occurrences,
  canHost,
  createOpen,
  onToggleCreate,
  error = null,
  pending = false,
  onRsvp,
  onWithdraw,
  onCreate,
  onRetire,
}: FdSessionsPageProps) {
  const [cadence, setCadence] = useState<"once" | "weekly">("weekly");

  const seriesCount = new Set(occurrences.map((o) => o.seriesId)).size;
  const totalRsvps = occurrences.reduce((sum, o) => sum + o.rsvpCount, 0);

  function submitCreate(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    // datetime-local values carry no zone: they are the host's LOCAL wall time.
    // Convert to an ISO instant here, where the browser knows its own zone —
    // submitting the bare string would let the server read it as UTC.
    const firstAtRaw = String(form.get("firstAt") ?? "");
    const firstAtDate = new Date(firstAtRaw);
    onCreate({
      title: String(form.get("title") ?? ""),
      body: String(form.get("body") ?? ""),
      sceneId: String(form.get("sceneId") ?? ""),
      cadence: String(form.get("cadence") ?? "") === "once" ? "once" : "weekly",
      firstAt: Number.isNaN(firstAtDate.getTime())
        ? firstAtRaw
        : firstAtDate.toISOString(),
      durationMinutes: String(form.get("durationMinutes") ?? ""),
    });
  }

  return (
    <div className="fd-page fd-stack fd-sessions">
      <FdPageHead
        title="The community calendar"
        aside={
          <Button
            variant="secondary"
            size="sm"
            disabled={!canHost}
            title={canHost ? undefined : "Hosting is granted by an invite code on People"}
            onClick={onToggleCreate}
          >
            {createOpen ? "Close the form" : "Schedule a session"}
          </Button>
        }
      />

      {!canHost ? (
        <p className="fd-note">
          Hosting is granted by an invite code on{" "}
          <a href="/foundry/people">People</a> — the button unlocks when a host
          role reaches this session.
        </p>
      ) : null}

      {/* An all-zero calendar is the empty state below, not a row of zeros. */}
      {occurrences.length > 0 || seriesCount > 0 || totalRsvps > 0 ? (
        <div className="fd-statrow">
          <FdStat
            label="On the calendar"
            value={occurrences.length}
            note="Dates inside the next 28 days."
          />
          <FdStat
            label="Active series"
            value={seriesCount}
            note="Schedules with a date still to come."
          />
          <FdStat
            label="RSVPs"
            value={totalRsvps}
            note="One per browser session per date."
          />
        </div>
      ) : null}

      {error ? (
        <p className="fd-alert" role="alert">
          {error}
        </p>
      ) : null}

      {canHost && createOpen ? (
        <form className="fd-form" method="post" onSubmit={submitCreate}>
          <h3 className="fd-form__title">Put a session on the calendar</h3>

          <div className="fd-form__field">
            <label className="fd-form__label" htmlFor="fd-session-title">
              Title
            </label>
            <input
              id="fd-session-title"
              className="fd-form__input"
              name="title"
              type="text"
              maxLength={80}
              required
              autoComplete="off"
            />
          </div>

          <div className="fd-form__field">
            <label className="fd-form__label" htmlFor="fd-session-body">
              What happens in this session
            </label>
            <textarea
              id="fd-session-body"
              className="fd-form__textarea"
              name="body"
              maxLength={280}
              rows={3}
            />
          </div>

          <div className="fd-form__field">
            <label className="fd-form__label" htmlFor="fd-session-scene">
              Scene id (optional)
            </label>
            <input
              id="fd-session-scene"
              className="fd-form__input"
              name="sceneId"
              type="text"
              autoComplete="off"
              placeholder="Leave blank if the session has no scene"
            />
          </div>

          <div className="fd-form__row">
            <div className="fd-form__field">
              <label className="fd-form__label" htmlFor="fd-session-cadence">
                Cadence
              </label>
              <select
                id="fd-session-cadence"
                className="fd-form__select"
                name="cadence"
                value={cadence}
                onChange={(e) => setCadence(e.target.value === "once" ? "once" : "weekly")}
              >
                <option value="weekly">Weekly</option>
                <option value="once">Once</option>
              </select>
            </div>

            <div className="fd-form__field">
              <label className="fd-form__label" htmlFor="fd-session-first">
                First occurrence (your local time)
              </label>
              <input
                id="fd-session-first"
                className="fd-form__input"
                name="firstAt"
                type="datetime-local"
                required
              />
            </div>

            <div className="fd-form__field">
              <label className="fd-form__label" htmlFor="fd-session-duration">
                Duration (minutes)
              </label>
              <input
                id="fd-session-duration"
                className="fd-form__input"
                name="durationMinutes"
                type="number"
                min={15}
                max={480}
                defaultValue={60}
                required
              />
            </div>
          </div>

          <div className="fd-form__actions">
            <Button type="submit" variant="primary" size="sm" disabled={pending}>
              Schedule it
            </Button>
            {pending ? <Spinner size={16} /> : null}
            <span className="fd-note-inline">
              {cadence === "weekly"
                ? "A weekly series repeats every 7 days."
                : "A one-time session falls off the calendar after it passes."}
            </span>
          </div>
        </form>
      ) : null}

      <FdSection
        title="Upcoming"
        badge={
          occurrences.length > 0 ? (
            <span className="fd-chip">
              {plural(occurrences.length, "date")}
            </span>
          ) : undefined
        }
      >
        {occurrences.length === 0 ? (
          <p className="fd-empty">No sessions on the calendar.</p>
        ) : (
          <div className="fd-board">
            {occurrences.map((o) => (
              // The identifiers the RSVP form posts are stamped on the DOM, so
              // a script or assistive tool can act on a card without parsing
              // the hydration payload; the button aria-labels carry the title
              // in straight quotes — the curly apostrophe in the visible label
              // has defeated text-matching automation before.
              <article
                className="fd-card"
                key={`${o.seriesId}#${o.occurrenceAt}`}
                data-series-id={o.seriesId}
                data-occurrence-at={o.occurrenceAt}
              >
                <h3 className="fd-card__title">{o.title}</h3>

                <p className="fd-session__when">
                  <FdTime iso={o.occurrenceAt} title={o.occurrenceAt}>
                    {stampUTC(o.occurrenceAt)}
                  </FdTime>
                </p>

                <p className="fd-chiprow">
                  <span className="fd-chip fd-chip--mono" title={o.label}>
                    {o.cadence}
                  </span>
                  <span className="fd-chip">{durationLabel(o.durationMinutes)}</span>
                  <FdPersonaChip actor={hostChip(o.host)} />
                  {o.sceneTitle ? (
                    o.sceneId ? (
                      <a className="fd-chip" href={`/foundry/play/${o.sceneId}`}>
                        {o.sceneTitle}
                      </a>
                    ) : (
                      <span className="fd-chip">{o.sceneTitle}</span>
                    )
                  ) : null}
                </p>

                {o.body ? <p className="fd-session__text">{o.body}</p> : null}

                <div className="fd-card__foot">
                  <span className="fd-chip fd-num">
                    {plural(o.rsvpCount, "RSVP")}
                  </span>
                  {o.viewerRsvped ? (
                    <Button
                      variant="secondary"
                      size="sm"
                      disabled={pending}
                      aria-label={`Withdraw RSVP: ${o.title}`}
                      onClick={() =>
                        onWithdraw({
                          seriesId: o.seriesId,
                          occurrenceAt: o.occurrenceAt,
                        })
                      }
                    >
                      Withdraw
                    </Button>
                  ) : (
                    <Button
                      variant="primary"
                      size="sm"
                      disabled={pending}
                      aria-label={`RSVP: ${o.title}`}
                      onClick={() =>
                        onRsvp({
                          seriesId: o.seriesId,
                          occurrenceAt: o.occurrenceAt,
                        })
                      }
                    >
                      I&rsquo;ll come
                    </Button>
                  )}
                  {o.viewerRsvped ? (
                    <span className="fd-note-inline">you said you&rsquo;ll come</span>
                  ) : null}
                </div>

                {canHost ? (
                  <div className="fd-session__hostrow">
                    <RetireControl
                      pending={pending}
                      onRetire={() => onRetire(o.seriesId)}
                    />
                  </div>
                ) : null}
              </article>
            ))}
          </div>
        )}
      </FdSection>
    </div>
  );
}
