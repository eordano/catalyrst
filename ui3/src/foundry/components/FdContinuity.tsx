import { type FormEvent, useState } from "react";

import Button from "../../atoms/Button";
import Spinner from "../../atoms/Spinner";
import FdPersonaChip, { type FdChipActor } from "./FdPersonaChip";
import FdScrollTable from "./FdScrollTable";
import FdSection, { FD_STEWARDS_SUB } from "./FdSection";
import FdTime from "./FdTime";
import { dayUTC } from "../fmt";
import "./fdpersonachip.css";
import "./fdtable.css";
import "./fdcontinuity.css";

export type FdContinuitySummaryVM = {
  changelog: number;
  reports: number;
  reportsAll: number;
  episodes: number;
  docs: number;
  stewards: number;
};

export type FdSceneMemoryRowVM = {
  /** Public row address; the row links to /foundry/timeline/<eventId>. */
  eventId: string;
  at: string;
  actor: { name: string } | { badge: string } | { source: string };
  action: string;
  body: string;
  sourceNote: string;
};

export type FdStewardVM = {
  actor: { badge: string } | { name: string };
  basis: string;
  since: string;
  releasedAt: string | null;
  releaseReason: "self" | "transfer" | null;
  viaTransfer: boolean;
};

export type FdTransferVM = {
  id: string;
  from: { badge: string } | { name: string };
  note: string;
  status: "offered" | "accepted" | "revoked";
  effectiveStatus: "offered" | "accepted" | "revoked" | "expired";
  createdAt: string;
  expiresAt: string;
  acceptedAt: string | null;
  acceptedBy: { badge: string } | { name: string } | null;
};

export type FdContinuityProps = {
  slug: string;
  summary: FdContinuitySummaryVM;
  memory: readonly FdSceneMemoryRowVM[];
  stewards: { active: readonly FdStewardVM[]; past: readonly FdStewardVM[] };
  transfers: readonly FdTransferVM[];
  isSteward: boolean;
  exportHref: string;
  /** A freshly-minted, one-time succession link — shown exactly once, then gone. */
  mintedTransferUrl?: string | null;
  error?: string | null;
  pending?: boolean;
  onClaim: (basis: string) => void;
  onRelease: () => void;
  onNote: (note: string) => void;
  onOfferTransfer: (note: string) => void;
  onRevokeTransfer: (transferId: string) => void;
};

function actorChip(actor: { badge: string } | { name: string } | { source: string }): FdChipActor {
  return actor as FdChipActor;
}

export default function FdContinuity({
  slug,
  summary,
  memory,
  stewards,
  transfers,
  isSteward,
  exportHref,
  mintedTransferUrl = null,
  error = null,
  pending = false,
  onClaim,
  onRelease,
  onNote,
  onOfferTransfer,
  onRevokeTransfer,
}: FdContinuityProps) {
  const [claimOpen, setClaimOpen] = useState(false);

  function submitClaim(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    onClaim(String(form.get("basis") ?? "").trim());
    setClaimOpen(false);
  }

  function submitNote(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    const el = event.currentTarget;
    onNote(String(form.get("note") ?? "").trim());
    el.reset();
  }

  function submitOffer(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    const el = event.currentTarget;
    onOfferTransfer(String(form.get("note") ?? "").trim());
    el.reset();
  }

  return (
    <>
      <FdSection title="Scene memory">
        <div className="fd-cont__summary fd-panel">
          {(() => {
            // All-zero renders nothing (the sections below carry the empty
            // states); once anything is nonzero every measured count renders,
            // zeros included — a measured zero is a fact, not noise.
            const any =
              summary.changelog > 0 ||
              summary.reports > 0 ||
              summary.episodes > 0 ||
              summary.docs > 0 ||
              summary.stewards > 0;
            if (!any) return null;
            const entries = [
              `${summary.changelog} changelog ${summary.changelog === 1 ? "entry" : "entries"}`,
              `${summary.reports} ${summary.reports === 1 ? "report" : "reports"} (sandbox sims excluded)`,
              `${summary.episodes} ${summary.episodes === 1 ? "episode" : "episodes"} (run logs, sandbox sims included)`,
              `${summary.docs} ${summary.docs === 1 ? "doc" : "docs"}`,
              `${summary.stewards} ${summary.stewards === 1 ? "steward" : "stewards"}`,
            ];
            return <p className="fd-cont__counts">{entries.join(" · ")}</p>;
          })()}
          <p className="fd-cont__exportnote">
            The stored bundle contains no scene bytes and cannot redeploy the
            live scene.
            {summary.reportsAll > 0 ? (
              <>
                {" "}
                It carries {summary.reportsAll === 1
                  ? "the 1 stored bench run"
                  : `all ${summary.reportsAll} stored bench runs`}
                , sandbox sims included.
              </>
            ) : null}
          </p>
          <a
            className="fd-cont__export"
            href={exportHref}
            download={`${slug}-foundry-bundle.json`}
          >
            Download the scene-memory bundle (JSON) — carries this scene&apos;s
            memory into the next build session.
          </a>
        </div>
      </FdSection>

      {error ? (
        <p className="fd-alert" role="alert">
          {error}
        </p>
      ) : null}

      <FdSection title="What happened, newest first">
        {memory.length === 0 ? (
          <p className="fd-empty">Nothing recorded for this scene yet.</p>
        ) : (
          <FdScrollTable ariaLabel="Scene memory">
            <table className="fd-table">
              <thead>
                <tr>
                  <th scope="col">When</th>
                  <th scope="col">Who</th>
                  <th scope="col">What</th>
                  <th scope="col">Source note</th>
                </tr>
              </thead>
              <tbody>
                {memory.map((m) => (
                  <tr key={m.eventId} className="fd-cont__memrow">
                    <td className="fd-table__mono">
                      <a
                        className="fd-cont__memlink"
                        href={`/foundry/timeline/${m.eventId}`}
                      >
                        <FdTime iso={m.at}>{dayUTC(m.at)}</FdTime>
                      </a>
                    </td>
                    <td>
                      <FdPersonaChip actor={actorChip(m.actor)} />
                    </td>
                    <td>
                      <span className="fd-table__sub">{m.action}</span>
                      {m.body ? <span> {m.body}</span> : null}
                    </td>
                    <td className="fd-table__sub">{m.sourceNote || "—"}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </FdScrollTable>
        )}
      </FdSection>

      <FdSection title="Stewards" sub={FD_STEWARDS_SUB}>
        {stewards.active.length === 0 && stewards.past.length === 0 ? (
          <p className="fd-empty">No steward has claimed this scene.</p>
        ) : (
          <ul className="fd-cont__stewards">
            {stewards.active.map((s, i) => (
              <li className="fd-cont__steward" key={`a${i}`}>
                <FdPersonaChip actor={actorChip(s.actor)} />
                <span className="fd-cont__stewardsince">
                  since <FdTime iso={s.since}>{dayUTC(s.since)}</FdTime>
                </span>
                {s.viaTransfer ? <span className="fd-chip">via transfer</span> : null}
                {s.basis ? <p className="fd-cont__basis">{s.basis}</p> : null}
              </li>
            ))}
            {stewards.past.map((s, i) => (
              <li className="fd-cont__steward is-past" key={`p${i}`}>
                <FdPersonaChip actor={actorChip(s.actor)} />
                <span className="fd-cont__stewardsince">
                  <FdTime iso={s.since}>{dayUTC(s.since)}</FdTime>
                  {s.releasedAt ? (
                    <>
                      {" – released "}
                      <FdTime iso={s.releasedAt}>{dayUTC(s.releasedAt)}</FdTime>
                    </>
                  ) : null}
                  {s.releaseReason ? ` (${s.releaseReason})` : ""}
                </span>
              </li>
            ))}
          </ul>
        )}

        <div className="fd-cont__stewardctl">
          {isSteward ? (
            <form method="post" onSubmit={submitNote} className="fd-cont__noteform">
              <label className="fd-cont__label" htmlFor="fd-scene-note">
                Leave a note on this scene
              </label>
              <textarea
                id="fd-scene-note"
                className="fd-cont__textarea"
                maxLength={280}
                name="note"
                rows={2}
                required
              />
              <Button type="submit" variant="secondary" size="sm" disabled={pending}>
                Add note
              </Button>
            </form>
          ) : claimOpen ? (
            <form method="post" onSubmit={submitClaim} className="fd-cont__noteform">
              <label className="fd-cont__label" htmlFor="fd-steward-basis">
                On what basis do you steward this scene? (optional — your own
                statement)
              </label>
              <input
                id="fd-steward-basis"
                className="fd-cont__input"
                maxLength={280}
                name="basis"
                type="text"
                autoComplete="off"
              />
              <p className="fd-note">
                This is your own statement. It is recorded as a claim, not checked against
                any deployment.
              </p>
              <div className="fd-cont__actions">
                <Button type="submit" variant="primary" size="sm" disabled={pending}>
                  Record my claim
                </Button>
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  onClick={() => setClaimOpen(false)}
                >
                  Cancel
                </Button>
              </div>
            </form>
          ) : (
            <Button variant="secondary" size="sm" onClick={() => setClaimOpen(true)}>
              Claim stewardship
            </Button>
          )}
          {isSteward ? (
            <Button variant="ghost" size="sm" onClick={onRelease} disabled={pending}>
              Release my stewardship
            </Button>
          ) : null}
          {pending ? <Spinner size={16} /> : null}
        </div>
      </FdSection>

      {isSteward || transfers.length > 0 ? (
        <FdSection title="Transfers">
          {mintedTransferUrl ? (
            <div className="fd-cont__minted fd-panel" role="status">
              <p className="fd-cont__mintedlead">
                Your one-time succession link — copy it now, it will not be shown again:
              </p>
              <code className="fd-cont__mintedurl">{mintedTransferUrl}</code>
            </div>
          ) : null}

          {isSteward ? (
            <form method="post" onSubmit={submitOffer} className="fd-cont__noteform">
              <label className="fd-cont__label" htmlFor="fd-transfer-note">
                Offer a stewardship transfer
              </label>
              <input
                id="fd-transfer-note"
                className="fd-cont__input"
                maxLength={280}
                name="note"
                type="text"
                autoComplete="off"
                placeholder="a note for whoever accepts"
              />
              <Button type="submit" variant="secondary" size="sm" disabled={pending}>
                Create transfer link
              </Button>
            </form>
          ) : null}

          {transfers.length === 0 ? (
            <p className="fd-note">No transfer has been offered for this scene.</p>
          ) : (
            <FdScrollTable ariaLabel="Transfers">
              <table className="fd-table">
                <thead>
                  <tr>
                    <th scope="col">From</th>
                    <th scope="col">Offered</th>
                    <th scope="col">Status</th>
                    <th scope="col">Accepted by</th>
                    <th scope="col"></th>
                  </tr>
                </thead>
                <tbody>
                  {transfers.map((t) => (
                    <tr key={t.id}>
                      <td>
                        <FdPersonaChip actor={actorChip(t.from)} />
                        {t.note ? <span className="fd-table__sub">{t.note}</span> : null}
                      </td>
                      <td className="fd-table__mono">
                        <FdTime iso={t.createdAt}>{dayUTC(t.createdAt)}</FdTime>
                      </td>
                      <td>
                        <span className="fd-chip">{t.effectiveStatus}</span>
                      </td>
                      <td>
                        {t.acceptedBy ? (
                          <FdPersonaChip actor={actorChip(t.acceptedBy)} />
                        ) : (
                          "—"
                        )}
                      </td>
                      <td>
                        {isSteward && t.effectiveStatus === "offered" ? (
                          <Button
                            variant="ghost"
                            size="sm"
                            onClick={() => onRevokeTransfer(t.id)}
                            disabled={pending}
                          >
                            Revoke
                          </Button>
                        ) : null}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </FdScrollTable>
          )}
        </FdSection>
      ) : null}
    </>
  );
}
