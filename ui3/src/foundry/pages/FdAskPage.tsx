import { useState, type FormEvent } from "react";

import Button from "../../atoms/Button";
import Spinner from "../../atoms/Spinner";
import {
  JOB_NAMES,
  type FdEmotionalJobLetter,
} from "../components/FdEmotionalJobs";
import {
  MARKET_CELL_NAMES,
  type FdMarketCellSlug,
} from "../components/FdGameCard";
import FdPersonaChip, { type FdChipActor } from "../components/FdPersonaChip";
import FdRequestCard, { type FdRequestCardVM } from "../components/FdRequestCard";
import FdSection, { FdPageHead } from "../components/FdSection";
import FdTime from "../components/FdTime";
import { FD_BODY_MAX } from "./FdExchangePage";
import { dayUTC } from "../fmt";
import "../components/fdcellchip.css";
import "./fdask.css";

export type FdAskPledgeRow = { actor: FdChipActor; at: string };

/** This program's own reading of the ask against the deck's cells and jobs —
 *  a dated judgment, never a fact the ask carries. Null = not yet read. */
export type FdAskReadingVM = {
  cell: FdMarketCellSlug | null;
  jobs: readonly FdEmotionalJobLetter[];
  shelfAnswer: { sceneId: string; title: string } | null;
  rationale: string;
  confidence: "evidence-backed" | "inferred";
  readAt: string;
  basis: string;
  /** The deck's own per-cell crowd range (slide 09), stored on the reading
   *  row; null when the reading fits no cell. */
  crowdRange: string | null;
};

export type FdAskEditErrors = Partial<Record<"title" | "body", string>>;

export type FdAskPageProps = {
  ask: FdRequestCardVM;
  reading: FdAskReadingVM | null;
  /** Program briefs whose stored grounding keys name this ask. */
  quotedInBriefs?: readonly { id: string; kind: string }[];
  pledgeList: readonly FdAskPledgeRow[];
  onPledge: () => void;
  onWithdraw: () => void;
  viewerIsAdmin?: boolean;
  onModerate?: (verdict: "approved" | "closed") => void;
  /** Present only when the viewer authored this ask. open/onToggle live with
   *  the route so a successful save can close the form. */
  edit?: {
    open: boolean;
    onToggle: () => void;
    onSave: (values: { title: string; body: string }) => void;
    errors: FdAskEditErrors;
  } | null;
  pending?: boolean;
  error?: string | null;
  backHref: string;
};

function readingProvenance(reading: FdAskReadingVM): string {
  return `This program's reading, ${reading.readAt} — ${reading.rationale} (${reading.confidence})`;
}

// Its own component so the draft dies with the form (the FdPostRequestForm
// pattern): closing it — or a successful save, which closes it — unmounts and
// clears, while a failed submit keeps the text intact.
function FdEditAskForm({
  defaultTitle,
  defaultBody,
  errors,
  error,
  pending,
  onSave,
}: {
  defaultTitle: string;
  defaultBody: string;
  errors: FdAskEditErrors;
  error: string | null;
  pending: boolean;
  onSave: (values: { title: string; body: string }) => void;
}) {
  const [body, setBody] = useState(defaultBody);
  const [overLimit, setOverLimit] = useState(false);

  function submitEdit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    const bodyText = String(form.get("body") ?? "");
    if (bodyText.length > FD_BODY_MAX) {
      setOverLimit(true);
      return;
    }
    setOverLimit(false);
    onSave({ title: String(form.get("title") ?? ""), body: bodyText });
  }

  const bodyError = overLimit
    ? `Keep the description under ${FD_BODY_MAX} characters — it is ${body.length} now. Nothing was saved; your text is kept.`
    : errors.body ?? null;
  const actionAlert = error ?? bodyError ?? errors.title ?? null;

  return (
    <form className="fd-form" method="post" onSubmit={submitEdit}>
      <h3 className="fd-form__title">Edit your ask</h3>

      <div className="fd-form__field">
        <label className="fd-form__label" htmlFor="fd-ask-edit-title">
          Title
        </label>
        <input
          id="fd-ask-edit-title"
          className="fd-form__input"
          name="title"
          type="text"
          maxLength={80}
          required
          autoComplete="off"
          defaultValue={defaultTitle}
        />
        {errors.title ? (
          <p className="fd-alert" role="alert">
            {errors.title}
          </p>
        ) : null}
      </div>

      <div className="fd-form__field">
        <div className="fd-form__labelrow">
          <label className="fd-form__label" htmlFor="fd-ask-edit-body">
            What is missing, and what would you show up for
          </label>
          <span
            id="fd-ask-edit-body-count"
            className={
              "fd-form__count" + (body.length > FD_BODY_MAX ? " is-over" : "")
            }
          >
            {body.length} / {FD_BODY_MAX}
          </span>
        </div>
        {/* No maxLength: a silently truncated paste loses words with no
            trace. The counter and the submit check carry the limit. */}
        <textarea
          id="fd-ask-edit-body"
          className="fd-form__textarea"
          name="body"
          rows={3}
          required
          value={body}
          onChange={(e) => {
            setBody(e.target.value);
            if (overLimit && e.target.value.length <= FD_BODY_MAX)
              setOverLimit(false);
          }}
          aria-describedby="fd-ask-edit-body-count"
        />
        {bodyError ? (
          <p className="fd-alert" role="alert">
            {bodyError}
          </p>
        ) : null}
      </div>

      <div className="fd-form__actions">
        <Button type="submit" variant="primary" size="sm" disabled={pending}>
          Save the edit
        </Button>
        {pending ? <Spinner size={16} /> : null}
        {actionAlert ? (
          <span className="fd-alert fd-form__alert" role="alert">
            {actionAlert}
          </span>
        ) : (
          <span className="fd-note-inline">
            Pledges stay; readers will see the ask marked as edited.
          </span>
        )}
      </div>
    </form>
  );
}

/** One ask, at its own shareable address. The card is the same one the board
 *  renders — same copy, same pledge flow — plus the full pledge list. */
export default function FdAskPage({
  ask,
  reading,
  quotedInBriefs,
  pledgeList,
  onPledge,
  onWithdraw,
  viewerIsAdmin = false,
  onModerate,
  edit = null,
  pending = false,
  error = null,
  backHref,
}: FdAskPageProps) {
  return (
    <div className="fd-page fd-stack fd-ask">
      <FdPageHead
        eyebrow="Exchange"
        title={ask.title}
        crumbs={<a href={backHref}>← All requests</a>}
      />

      {error ? (
        <p className="fd-alert" role="alert">
          {error}
        </p>
      ) : null}

      <FdSection
        title="The request"
        aside={
          edit ? (
            <Button
              variant="ghost"
              size="sm"
              onClick={edit.onToggle}
              disabled={pending}
            >
              {edit.open ? "Close the editor" : "Edit your ask"}
            </Button>
          ) : undefined
        }
      >
        <FdRequestCard
          {...ask}
          // The full reading section renders directly below — a chip on the card
          // would duplicate it. The page head already carries the title, so the
          // card's self-link would only point at the page it is on.
          reading={undefined}
          linkTitle={false}
          onPledge={onPledge}
          onWithdraw={onWithdraw}
          viewerIsAdmin={viewerIsAdmin}
          onModerate={onModerate}
          pending={pending}
        />
        {edit?.open ? (
          <FdEditAskForm
            defaultTitle={ask.title}
            defaultBody={ask.body}
            errors={edit.errors}
            error={error}
            pending={pending}
            onSave={edit.onSave}
          />
        ) : null}
      </FdSection>

      <FdSection id="reading" title="This program’s reading">
        {reading === null ? (
          <p className="fd-empty">
            Not yet read against the deck&rsquo;s market cells.
          </p>
        ) : (
          <>
            <p className="fd-chiprow">
              <a
                className="fd-cellchip"
                href="/foundry/deck#slide-09"
                title={readingProvenance(reading)}
              >
                {reading.cell !== null
                  ? MARKET_CELL_NAMES[reading.cell]
                  : "Fits none of the three cells"}
              </a>
              {reading.jobs.map((j) => (
                <a
                  key={j}
                  className="fd-cellchip"
                  href="/foundry/deck#slide-10"
                  title={readingProvenance(reading)}
                >
                  {JOB_NAMES[j]}
                </a>
              ))}
              <span className="fd-chip" title={reading.basis}>
                {reading.confidence}
              </span>
              <FdTime
                iso={reading.readAt}
                className="fd-chip fd-chip--mono"
                title={reading.basis}
              >
                read {reading.readAt}
              </FdTime>
              <span className="u-sr-only">{readingProvenance(reading)}</span>
            </p>

            <p className="fd-panelnote">{reading.rationale}</p>

            {reading.jobs.length === 0 ? (
              <p className="fd-panelnote">
                None of the deck&rsquo;s{" "}
                <a href="/foundry/deck#slide-10">six emotional jobs</a> was read
                from this ask.
              </p>
            ) : null}

            <p className="fd-panelnote">
              {reading.shelfAnswer !== null ? (
                <>
                  On the shelf today:{" "}
                  <a href={`/foundry/play/${reading.shelfAnswer.sceneId}`}>
                    {reading.shelfAnswer.title}
                  </a>
                  .
                </>
              ) : (
                <>
                  No game on the shelf answers this ask —{" "}
                  <a
                    href="/foundry/play"
                    aria-label="open ground on the games shelf"
                  >
                    open ground
                  </a>
                  .
                </>
              )}
            </p>

            {reading.cell !== null && reading.crowdRange !== null ? (
              <p className="fd-panelnote">
                The deck sizes a live {MARKET_CELL_NAMES[reading.cell]} cell at
                &ldquo;{reading.crowdRange}&rdquo; (
                <a href="/foundry/deck#slide-09">deck slide 09</a>).
              </p>
            ) : null}

            {(quotedInBriefs ?? []).map((b) => (
              <p className="fd-panelnote" key={b.id}>
                <a href={`/foundry/gdd/${b.id}`}>
                  A {b.kind === "brief" ? "brief" : "design doc"} in the
                  design docs is grounded on this ask →
                </a>
              </p>
            ))}
          </>
        )}
      </FdSection>

      <FdSection title="Pledges">
        {pledgeList.length === 0 ? (
          <p className="fd-empty">No pledges yet.</p>
        ) : (
          <ul className="fd-ask__pledges">
            {pledgeList.map((p, i) => (
              <li className="fd-ask__pledge" key={i}>
                <FdPersonaChip actor={p.actor} />
                <FdTime iso={p.at} className="fd-ask__pledgeat" title={p.at}>
                  {dayUTC(p.at)}
                </FdTime>
              </li>
            ))}
          </ul>
        )}
      </FdSection>
    </div>
  );
}
