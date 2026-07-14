import { type FormEvent, type ReactNode, useState } from "react";

import { plural, stampUTC } from "../fmt";
import Button from "../../atoms/Button";
import Spinner from "../../atoms/Spinner";
import FdPersonaChip from "../components/FdPersonaChip";
import FdScrollTable from "../components/FdScrollTable";
import FdSection, { FdPageHead } from "../components/FdSection";
import FdTime from "../components/FdTime";
import "../components/fdtable.css";
import "../components/fdpersonachip.css";
import "./fdstewardship.css";

export type FdConsentTopicId = "steward-code" | "roster-listing";

export type FdConsentEntryVM = {
  state: "granted" | "withdrawn";
  at: string;
} | null;

export type FdStewardConsentVM = {
  stewardCode: FdConsentEntryVM;
  rosterListing: FdConsentEntryVM;
};

export type FdDecisionKind = "request" | "role_grant" | "session_series";

export type FdDecisionVM = {
  kind: FdDecisionKind;
  id: string;
  label: string;
  at: string;
  detail: string;
};

export type FdAppealActor = { name: string } | { badge: string };

export type FdAppealVM = {
  id: string;
  subjectKind: FdDecisionKind;
  subjectId: string;
  subjectLabel: string | null;
  body: string;
  status: "open" | "withdrawn" | "upheld" | "declined";
  createdAt: string;
  resolvedBy: FdAppealActor | null;
  resolvedAt: string | null;
  resolutionNote: string | null;
  appellant?: FdAppealActor;
};

export type FdStewardshipPageProps = {
  consent: FdStewardConsentVM;
  decisions: readonly FdDecisionVM[];
  myAppeals: readonly FdAppealVM[];
  isAdmin: boolean;
  openAppeals: readonly FdAppealVM[];
  bodyLimit: number;
  error?: string | null;
  pending?: boolean;
  onConsent: (values: { topic: FdConsentTopicId; state: "granted" | "withdrawn" }) => void;
  onFile: (values: { subjectKind: FdDecisionKind; subjectId: string; body: string }) => void;
  onWithdraw: (appealId: string) => void;
  onResolve: (values: {
    appealId: string;
    verdict: "upheld" | "declined";
    note: string;
  }) => void;
};

const KIND_LABEL: Record<FdDecisionKind, string> = {
  request: "request",
  role_grant: "role grant",
  session_series: "session series",
};

const STATUS_LABEL: Record<FdAppealVM["status"], string> = {
  open: "open",
  withdrawn: "withdrawn",
  upheld: "upheld",
  declined: "declined",
};

const CODE_CLAUSES: readonly { title: string; body: ReactNode }[] = [
  {
    title: "Hosting runs on standing consent.",
    body: "Withdraw it and host powers stop that instant — nothing waits for a moderator.",
  },
  {
    title: "Naming is opt-in.",
    body: (
      <>
        A role holder appears on the <a href="/foundry/people">People roster</a>{" "}
        only while roster-listing consent stands. Withdrawn means counted, never
        named.
      </>
    ),
  },
  {
    title: "Consequential acts are remembered.",
    body: "Grants, revocations, retirements, appeals and consent changes all leave a trace.",
  },
  {
    title: "Every decision can be contested.",
    body: "An appeal names a decision that touched you.",
  },
  {
    title: "Resolutions carry reasons.",
    body: "Only an operator can resolve an appeal, never without a written note. The appellant reads the note with the verdict.",
  },
];

function ConsentControl({
  topic,
  title,
  description,
  entry,
  pending,
  onConsent,
}: {
  topic: FdConsentTopicId;
  title: string;
  description: ReactNode;
  entry: FdConsentEntryVM;
  pending: boolean;
  onConsent: FdStewardshipPageProps["onConsent"];
}) {
  const granted = entry?.state === "granted";
  const next = granted ? "withdrawn" : "granted";
  function submitConsent(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    onConsent({ topic, state: next });
  }
  return (
    <div className="fd-card">
      <h3 className="fd-card__title">{title}</h3>
      <p className="fd-chiprow">
        {entry ? (
          <span
            className={
              "fd-chip fd-chip--mono" + (granted ? " fd-stew__granted" : "")
            }
          >
            {entry.state}
            {" · "}
            <FdTime iso={entry.at} title={entry.at}>
              {stampUTC(entry.at)}
            </FdTime>
          </span>
        ) : (
          <span className="fd-chip fd-chip--mono">no record yet</span>
        )}
      </p>
      <p className="fd-stew__consentbody">{description}</p>
      <form className="fd-card__foot" method="post" onSubmit={submitConsent}>
        <input type="hidden" name="intent" value="consent" />
        <input type="hidden" name="topic" value={topic} />
        <input type="hidden" name="state" value={next} />
        <Button
          type="submit"
          variant={granted ? "secondary" : "primary"}
          size="sm"
          disabled={pending}
        >
          {granted ? "Withdraw consent" : "Grant consent"}
        </Button>
      </form>
    </div>
  );
}

function AppealCard({
  appeal,
  pending,
  onWithdraw,
  onResolve,
}: {
  appeal: FdAppealVM;
  pending: boolean;
  onWithdraw?: (appealId: string) => void;
  onResolve?: FdStewardshipPageProps["onResolve"];
}) {
  const a = appeal;
  function submitResolve(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!onResolve) return;
    const form = new FormData(event.currentTarget);
    const verdict = String(form.get("verdict") ?? "") === "upheld" ? "upheld" : "declined";
    onResolve({
      appealId: a.id,
      verdict,
      note: String(form.get("note") ?? "").trim(),
    });
  }
  return (
    <article className="fd-card">
      <h3 className="fd-card__title">
        {a.subjectLabel ?? "A decision that no longer resolves"}
      </h3>
      <p className="fd-chiprow">
        <span className="fd-chip">{KIND_LABEL[a.subjectKind]}</span>
        <span
          className={
            "fd-chip fd-chip--mono" + (a.status === "open" ? " fd-stew__open" : "")
          }
        >
          {STATUS_LABEL[a.status]}
        </span>
      </p>
      <p className="fd-stew__body">{a.body}</p>
      <p className="fd-chiprow">
        <FdTime iso={a.createdAt} className="fd-note-inline" title={a.createdAt}>
          filed {stampUTC(a.createdAt)}
        </FdTime>
        {a.appellant ? <FdPersonaChip actor={a.appellant} /> : null}
      </p>
      {a.resolvedAt ? (
        <div className="fd-stew__resolution">
          <p className="fd-chiprow">
            <FdTime
              iso={a.resolvedAt}
              className="fd-note-inline"
              title={a.resolvedAt}
            >
              {STATUS_LABEL[a.status]} {stampUTC(a.resolvedAt)}
            </FdTime>
            {a.resolvedBy ? <FdPersonaChip actor={a.resolvedBy} /> : null}
          </p>
          {a.resolutionNote ? (
            <p className="fd-stew__note">{a.resolutionNote}</p>
          ) : null}
        </div>
      ) : null}
      {a.status === "open" && onWithdraw ? (
        <div className="fd-card__foot">
          <Button
            variant="secondary"
            size="sm"
            disabled={pending}
            onClick={() => onWithdraw(a.id)}
          >
            Withdraw appeal
          </Button>
        </div>
      ) : null}
      {a.status === "open" && onResolve ? (
        <form className="fd-stew__resolve" onSubmit={submitResolve}>
          <div className="fd-form__field">
            <label className="fd-form__label" htmlFor={`fd-verdict-${a.id}`}>
              Verdict
            </label>
            <select
              id={`fd-verdict-${a.id}`}
              className="fd-form__select fd-stew__verdict"
              name="verdict"
              defaultValue="declined"
            >
              <option value="upheld">Upheld</option>
              <option value="declined">Declined</option>
            </select>
          </div>
          <div className="fd-form__field">
            <label className="fd-form__label" htmlFor={`fd-note-${a.id}`}>
              Resolution note (shown to the appellant)
            </label>
            <input
              id={`fd-note-${a.id}`}
              className="fd-form__input"
              name="note"
              type="text"
              required
              autoComplete="off"
            />
          </div>
          <div className="fd-form__actions">
            <Button type="submit" variant="primary" size="sm" disabled={pending}>
              Resolve
            </Button>
          </div>
        </form>
      ) : null}
    </article>
  );
}

export default function FdStewardshipPage({
  consent,
  decisions,
  myAppeals,
  isAdmin,
  openAppeals,
  bodyLimit,
  error = null,
  pending = false,
  onConsent,
  onFile,
  onWithdraw,
  onResolve,
}: FdStewardshipPageProps) {
  const [subject, setSubject] = useState("");

  function submitFile(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    const raw = String(form.get("subject") ?? "");
    const sep = raw.indexOf(":");
    if (sep <= 0) return;
    const kind = raw.slice(0, sep);
    if (kind !== "request" && kind !== "role_grant" && kind !== "session_series") {
      return;
    }
    onFile({
      subjectKind: kind,
      subjectId: raw.slice(sep + 1),
      body: String(form.get("body") ?? "").trim(),
    });
  }

  return (
    <div className="fd-page fd-stack fd-stew">
      <FdPageHead title="The steward code" />

      {error ? (
        <p className="fd-alert" role="alert">
          {error}
        </p>
      ) : null}

      <FdSection title="The code, clause by clause">
        <ol className="fd-stew__code">
          {CODE_CLAUSES.map((c) => (
            <li key={c.title} className="fd-stew__clause">
              <span className="fd-stew__clausetitle">{c.title}</span>{" "}
              <span className="fd-stew__clausebody">{c.body}</span>
            </li>
          ))}
        </ol>
      </FdSection>

      <FdSection title="Your consents">
        <div className="fd-board">
          <ConsentControl
            topic="steward-code"
            title="Steward-code consent"
            description={
              <>
                Assent to the code above. Required before this session can redeem
                a <a href="/foundry/people">host invite</a> or take any hosting
                act.
              </>
            }
            entry={consent.stewardCode}
            pending={pending}
            onConsent={onConsent}
          />
          <ConsentControl
            topic="roster-listing"
            title="Roster-listing consent"
            description={
              <>
                Whether this session, if it holds a role, is named on the{" "}
                <a href="/foundry/people">People roster</a>.
              </>
            }
            entry={consent.rosterListing}
            pending={pending}
            onConsent={onConsent}
          />
        </div>
      </FdSection>

      <FdSection
        title="Decisions touching this session"
        badge={
          decisions.length > 0 ? (
            <span className="fd-chip">{plural(decisions.length, "decision")}</span>
          ) : undefined
        }
      >
        {decisions.length === 0 ? (
          <p className="fd-empty">No decision has touched this session yet.</p>
        ) : (
          <>
            <FdScrollTable ariaLabel="Decisions touching this session">
              <table className="fd-table">
                <thead>
                  <tr>
                    <th scope="col">Kind</th>
                    <th scope="col">Decision</th>
                    <th scope="col">Outcome</th>
                    <th scope="col">When</th>
                  </tr>
                </thead>
                <tbody>
                  {decisions.map((d) => (
                    <tr key={`${d.kind}:${d.id}`}>
                      <td>
                        <span className="fd-chip">{KIND_LABEL[d.kind]}</span>
                      </td>
                      <th scope="row">{d.label}</th>
                      <td>{d.detail}</td>
                      <td className="fd-table__mono">
                        <FdTime iso={d.at} title={d.at}>
                          {stampUTC(d.at)}
                        </FdTime>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </FdScrollTable>

            <form className="fd-form fd-stew__form" method="post" onSubmit={submitFile}>
              <input type="hidden" name="intent" value="file" />
              <input
                type="hidden"
                name="subjectKind"
                value={subject.slice(0, Math.max(0, subject.indexOf(":")))}
              />
              <input
                type="hidden"
                name="subjectId"
                value={subject.slice(subject.indexOf(":") + 1)}
              />
              <h3 className="fd-form__title">File an appeal</h3>
              <div className="fd-form__field">
                <label className="fd-form__label" htmlFor="fd-appeal-subject">
                  The decision you are contesting
                </label>
                <select
                  id="fd-appeal-subject"
                  className="fd-form__select"
                  name="subject"
                  required
                  value={subject}
                  onChange={(e) => setSubject(e.target.value)}
                >
                  <option value="" disabled>
                    Choose a decision…
                  </option>
                  {decisions.map((d) => (
                    <option key={`${d.kind}:${d.id}`} value={`${d.kind}:${d.id}`}>
                      {KIND_LABEL[d.kind]} — {d.label} ({d.detail})
                    </option>
                  ))}
                </select>
              </div>
              <div className="fd-form__field">
                <label className="fd-form__label" htmlFor="fd-appeal-body">
                  What you are contesting, and why
                </label>
                <textarea
                  id="fd-appeal-body"
                  className="fd-form__textarea"
                  name="body"
                  maxLength={bodyLimit}
                  rows={3}
                  required
                />
              </div>
              <div className="fd-form__actions">
                <Button type="submit" variant="primary" size="sm" disabled={pending}>
                  File the appeal
                </Button>
                {pending ? <Spinner size={16} /> : null}
                <span className="fd-note-inline">
                  {bodyLimit} characters or fewer. One open appeal per decision.
                </span>
              </div>
            </form>
          </>
        )}
      </FdSection>

      <FdSection
        title="Your appeals"
        badge={
          myAppeals.length > 0 ? (
            <span className="fd-chip">{plural(myAppeals.length, "appeal")}</span>
          ) : undefined
        }
      >
        {myAppeals.length === 0 ? (
          <p className="fd-empty">You have not filed an appeal.</p>
        ) : (
          <div className="fd-board">
            {myAppeals.map((a) => (
              <AppealCard
                key={a.id}
                appeal={a}
                pending={pending}
                onWithdraw={onWithdraw}
              />
            ))}
          </div>
        )}
      </FdSection>

      {isAdmin ? (
        <FdSection
          title="Open appeals"
          badge={
            openAppeals.length > 0 ? (
              <span className="fd-chip">{openAppeals.length} waiting</span>
            ) : undefined
          }
          aside={<span className="fd-chip fd-chip--mono">operator</span>}
        >
          {openAppeals.length === 0 ? (
            <p className="fd-empty">No open appeals.</p>
          ) : (
            <div className="fd-board">
              {openAppeals.map((a) => (
                <AppealCard
                  key={a.id}
                  appeal={a}
                  pending={pending}
                  onResolve={onResolve}
                />
              ))}
            </div>
          )}
        </FdSection>
      ) : null}
    </div>
  );
}
