import type { ReactNode } from "react";
import Button from "../../atoms/Button";
import Spinner from "../../atoms/Spinner";
import { MARKET_CELL_NAMES, type FdMarketCellSlug } from "./FdGameCard";
import FdPersonaChip, { type FdChipActor } from "./FdPersonaChip";
import FdTime from "./FdTime";
import { dayUTC, plural } from "../fmt";
import "./fdcellchip.css";
import "./fdpersonachip.css";
import "./fdrequestcard.css";

export type FdRequestActor = { name: string } | { badge: string } | null;

export type FdRequestModeration = {
  state: "open" | "approved" | "closed";
  actor: { name: string } | { badge: string } | null;
  at: string | null;
};

export type FdRequestCardVM = {
  id: string;
  title: string;
  body: string;
  source: string;
  status: "open" | "closed";
  pledges: number;
  pledgedByMe: boolean;
  origin: "imported" | "visitor";
  author?: FdRequestActor;
  /** Whether the viewer authored this ask — drives the edit affordance on the
   *  ask's own page; the card itself renders nothing from it. */
  authoredByMe?: boolean;
  moderation?: FdRequestModeration;
  createdAt?: string;
  /** When the author last revised the wording; the card stamps it so a reader
   *  never mistakes a revised ask for its original wording. */
  editedAt?: string | null;
  sourceUrl?: string | null;
  sourcedAt?: string | null;
  /** This program's reading of the ask: null = no reading row (honest "not
   *  read"); undefined = the surface renders no reading line at all. */
  reading?: { cell: FdMarketCellSlug | null; readAt: string } | null;
};

type FdRequestCardProps = FdRequestCardVM & {
  onPledge: () => void;
  onWithdraw: () => void;
  viewerIsAdmin?: boolean;
  onModerate?: (verdict: "approved" | "closed") => void;
  pending?: boolean;
  /** false on the ask's own page, where the title would only link to itself. */
  linkTitle?: boolean;
};

function chipActor(actor: NonNullable<FdRequestActor>): FdChipActor {
  return "name" in actor ? { name: actor.name } : { badge: actor.badge };
}

// The origin chip on an imported ask is derived from the permalink itself,
// never a bare "imported" label: the working link is the proof.
function srcLabel(url: string): string | null {
  try {
    const host = new URL(url).hostname;
    if (host.includes("forum")) return "from the forum";
    if (host.includes("reddit")) return "from reddit";
    return host;
  } catch {
    return null;
  }
}

// A standalone " … " in a curated quote marks words left out of it; the
// author's own trailing punctuation never takes that shape. Marked so the
// reader can see the quote is not continuous.
function quoted(body: string): ReactNode {
  const parts = body.split(" … ");
  if (parts.length === 1) return body;
  return parts.map((part, i) => (
    <span key={i}>
      {i > 0 ? (
        <span className="fd-request__elide" title="words left out of the quote">
          {" … "}
        </span>
      ) : null}
      {part}
    </span>
  ));
}

function readingChipLabel(readAt: string): string {
  return `This program's reading, ${readAt} — the full reading is on the ask's page.`;
}

export default function FdRequestCard({
  id,
  title,
  body,
  source,
  pledges,
  status,
  pledgedByMe,
  origin,
  author,
  moderation,
  sourceUrl,
  sourcedAt,
  editedAt,
  reading,
  onPledge,
  onWithdraw,
  viewerIsAdmin = false,
  onModerate,
  pending = false,
  linkTitle = true,
}: FdRequestCardProps) {
  const open = status === "open";
  const state = moderation?.state ?? (open ? "open" : "closed");

  return (
    // The stable request id is the card's anchor (so /foundry/exchange#<id>
    // still lands on it); the title links to the ask's own shareable page.
    <article className="fd-card fd-request" id={id}>
      {linkTitle ? (
        <h3 className="fd-card__title">
          <a className="fd-request__self" href={`/foundry/exchange/${id}`}>
            {title}
          </a>
        </h3>
      ) : (
        // On the ask's own page the FdPageHead h1 already carries this exact
        // title — a second heading here would only duplicate the outline.
        <p className="fd-card__title">{title}</p>
      )}

      {/* On an imported ask, only the quote and its caption are the author's:
          the title above is this board's index entry for the ask, so the
          attribution (handle, date, permalink) is typeset as the quote's own
          caption rather than floating where it could claim the title too. */}
      {origin === "imported" ? (
        <figure className="fd-request__quotewrap">
          <blockquote
            className="fd-request__text fd-request__text--quote"
            cite={sourceUrl ?? undefined}
          >
            {quoted(body)}
          </blockquote>
          <figcaption className="fd-request__cite">
            — <span className="fd-request__source">{source}</span>
            {sourcedAt ? (
              <FdTime iso={sourcedAt} title={sourcedAt}>
                {" "}· {dayUTC(sourcedAt)}
              </FdTime>
            ) : null}
            {sourceUrl && srcLabel(sourceUrl) ? (
              <a
                className="fd-chip"
                href={sourceUrl}
                target="_blank"
                rel="noopener noreferrer"
              >
                {srcLabel(sourceUrl)}
              </a>
            ) : null}
          </figcaption>
        </figure>
      ) : (
        <p className="fd-request__text">{body}</p>
      )}

      {/* This program's reading of the ask, or its honest absence — the card
          title already links to the ask's page, where the full reading lives.
          `undefined` means the surface renders the reading elsewhere. */}
      {reading === null ? (
        <p className="fd-empty">Not yet read against the deck&rsquo;s market cells.</p>
      ) : reading !== undefined ? (
        <p className="fd-chiprow">
          <a
            className="fd-cellchip fd-cellchip--link"
            href={`/foundry/exchange/${id}#reading`}
            title={readingChipLabel(reading.readAt)}
            aria-label={readingChipLabel(reading.readAt)}
          >
            {reading.cell !== null
              ? MARKET_CELL_NAMES[reading.cell]
              : "Fits none of the three cells"}{" "}
            · <FdTime iso={reading.readAt}>{reading.readAt}</FdTime>
          </a>
        </p>
      ) : null}

      {origin !== "imported" || state !== "open" ? (
        <p className="fd-chiprow">
          {origin === "imported" ? null : (
            <>
              <span className="fd-note-inline">{source}</span>
              {author ? <FdPersonaChip actor={chipActor(author)} /> : null}
              {editedAt ? (
                <FdTime iso={editedAt} className="fd-chip" title={editedAt}>
                  edited {dayUTC(editedAt)}
                </FdTime>
              ) : null}
            </>
          )}
          {state === "approved" ? (
            <span className="fd-chip fd-request__approved">approved</span>
          ) : null}
          {state === "closed" ? (
            <span className="fd-chip">closed to new pledges</span>
          ) : null}
        </p>
      ) : null}

      {moderation && moderation.actor ? (
        <p className="fd-chiprow">
          <span className="fd-note-inline">
            {moderation.state === "approved" ? "approved by" : "closed by"}
          </span>
          <FdPersonaChip actor={chipActor(moderation.actor)} />
          {moderation.at ? (
            <FdTime
              iso={moderation.at}
              className="fd-note-inline"
              title={moderation.at}
            >
              {dayUTC(moderation.at)}
            </FdTime>
          ) : null}
        </p>
      ) : null}

      <div className="fd-card__foot">
        <span
          className="fd-chip fd-num"
          title="one pledge per browser session"
        >
          {plural(pledges, "pledge")}
        </span>
        {open ? (
          pledgedByMe ? (
            <Button variant="secondary" size="sm" onClick={onWithdraw} disabled={pending}>
              Withdraw pledge
            </Button>
          ) : (
            <Button variant="primary" size="sm" onClick={onPledge} disabled={pending}>
              Pledge to this request
            </Button>
          )
        ) : null}
        {viewerIsAdmin && onModerate ? (
          <>
            {state !== "approved" ? (
              <Button
                variant="secondary"
                size="sm"
                onClick={() => onModerate("approved")}
                disabled={pending}
              >
                Approve
              </Button>
            ) : null}
            {state !== "closed" ? (
              <Button
                variant="ghost"
                size="sm"
                onClick={() => onModerate("closed")}
                disabled={pending}
              >
                Close
              </Button>
            ) : null}
          </>
        ) : null}
        {pending ? <Spinner size={16} /> : null}
        {open && pledgedByMe ? (
          <span className="fd-note-inline">you pledged</span>
        ) : null}
      </div>
    </article>
  );
}
