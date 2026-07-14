import FdPersonaChip, { type FdChipActor } from "../components/FdPersonaChip";
import FdSection, { FdPageHead } from "../components/FdSection";
import FdTime from "../components/FdTime";
import { stampUTC } from "../fmt";
import "./fdtimelineevent.css";

export type FdTimelineEventVM = {
  eventId: string;
  kind: "changelog" | "action";
  at: string;
  actor: FdChipActor | null;
  action: string;
  body: string;
  sourceNote: string;
  origin: "import" | "visitor";
  scene: { title: string | null; href: string };
  /** The deployment entity this record derives from, when one exists — the
   *  content-server JSON in the worlds mirror. */
  entityHref: string | null;
};

/** Every stored action this page can resolve, in the words a reader uses. The
 *  stored enum still renders, as the record fact. */
const ACTION_TITLE: Record<string, string> = {
  changelog: "Changelog entry",
  claim_steward: "Stewardship claimed",
  release_steward: "Stewardship released",
  offer_transfer: "Stewardship offered",
  revoke_transfer: "Stewardship offer revoked",
  accept_transfer: "Stewardship accepted",
};

/** One remembered event, every field labeled with where it comes from. A field
 *  the source row does not hold is stated absent, never filled in. */
export default function FdTimelineEventPage({ event }: { event: FdTimelineEventVM }) {
  const sourceLabel =
    event.kind === "changelog"
      ? event.origin === "import"
        ? "the scene changelog, imported from the Worlds mirror"
        : "the scene changelog, written by a visitor on this site"
      : "this program's own action log";
  return (
    <div className="fd-page fd-stack fd-tlevent">
      <FdPageHead
        eyebrow="Timeline"
        title={ACTION_TITLE[event.action] ?? "Remembered event"}
        crumbs={<a href="/foundry/timeline">← All events</a>}
      />

      <FdSection title="What the row holds">
        <dl className="fd-facts fd-panel">
          <div>
            <dt>When</dt>
            <dd>
              <FdTime
                iso={event.at}
                title="the row's own timestamp"
                sr="the row's own timestamp"
              >
                {stampUTC(event.at)}
              </FdTime>
            </dd>
          </div>
          <div>
            <dt>Who</dt>
            <dd>
              {event.actor ? (
                <span title="the session that wrote the row, by claimed name or badge">
                  <FdPersonaChip actor={event.actor} />
                </span>
              ) : (
                <span className="fd-note-inline">no author recorded</span>
              )}
            </dd>
          </div>
          <div>
            <dt>Note</dt>
            <dd>
              {event.body || <span className="fd-note-inline">none</span>}
            </dd>
          </div>
          <div>
            <dt>Source note</dt>
            <dd>
              {event.sourceNote ? (
                <span className="fd-mono">{event.sourceNote}</span>
              ) : (
                <span className="fd-note-inline">none</span>
              )}
            </dd>
          </div>
          <div>
            <dt>Read from</dt>
            <dd>
              {sourceLabel}
              {event.entityHref ? (
                <>
                  {" · "}
                  <a href={event.entityHref} target="_blank" rel="noreferrer">
                    the deployment entity
                  </a>
                </>
              ) : null}
            </dd>
          </div>
          <div>
            <dt>Game</dt>
            <dd>
              <a href={event.scene.href}>
                {event.scene.title ?? event.scene.href}
              </a>
            </dd>
          </div>
          <div>
            <dt>Record</dt>
            <dd>
              <span
                className="fd-mono"
                title={
                  event.kind === "changelog"
                    ? "the stored kind and origin of this row"
                    : "the stored kind and action of this row"
                }
              >
                {event.kind} ·{" "}
                {event.kind === "changelog" ? event.origin : event.action}
              </span>
            </dd>
          </div>
        </dl>
      </FdSection>
    </div>
  );
}
