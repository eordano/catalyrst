import { useEffect, useMemo, useRef, useState } from "react";
import { useMachine } from "@xstate/react";
import { useSearchParams } from "react-router";

import GovernanceChrome, { type GovernanceNavId } from "@ui/governance/frames/GovernanceChrome";
import SectionIcon from "@ui/explorer/atoms/SectionIcon";
import "@ui/governance/pages/gveditprojectupdate.css";

import type { TrackContext } from "@core/lib/telemetry/track";
import type {
  EditUpdateData,
  FinancialRecord,
  ProjectHealth,
} from "@data/lib/catalyst/governance/edit-project-update";
import {
  editUpdateMachine,
  resolveEditSnapshot,
  slugToState,
  stateToSlug,
  type EditDraft,
  type SaveFn,
  type TrackFn,
} from "./machine";

export type GvEditProjectUpdateProps = {
  data: EditUpdateData;
  trackCtx: TrackContext;
  initialStep?: string;
  saveUpdate?: SaveFn;
  track?: TrackFn;
};

const HEALTHS: { id: ProjectHealth; label: string; cls: string }[] = [
  { id: "onTrack", label: "On Track", cls: "gpu__health--on" },
  { id: "atRisk", label: "At Risk", cls: "gpu__health--at" },
  { id: "offTrack", label: "Off Track", cls: "gpu__health--off" },
];

const GENERAL_FIELDS = [
  { id: "introduction", label: "Introduction", max: 500, minHeight: 77 },
  { id: "highlights", label: "Highlights", max: 3500, minHeight: 150 },
  { id: "blockers", label: "Blockers", max: 3500, minHeight: 120 },
  { id: "next_steps", label: "Next Steps", max: 3500, minHeight: 120 },
  {
    id: "additional_notes",
    label: "Additional notes and links",
    max: 3500,
    minHeight: 100,
  },
] as const;

const CSV_HEADER = ["category", "description", "token", "amount", "receiver", "link"];

const CURRENCY = (n: number) =>
  "$" + Number(n).toLocaleString("en-US", { maximumFractionDigits: 2 });

function recordsToCsv(records: FinancialRecord[]): string {
  const lines = [CSV_HEADER.join(",")];
  for (const r of records) {
    lines.push(
      CSV_HEADER.map((k) => String((r as Record<string, unknown>)[k] ?? "")).join(","),
    );
  }
  return lines.join("\n");
}

function SectionHead({
  n,
  title,
  isNew,
  validated,
}: {
  n: number;
  title: string;
  isNew?: boolean;
  validated?: boolean;
}) {
  return (
    <div className="gpu__sectionhead">
      <SectionIcon n={n} number={n} validated={validated} className="gpu__sicon" />
      <div className="gpu__sectiontitle">
        {title}
        {isNew && <span className="gpu__newbadge">New</span>}
      </div>
      <div className="gpu__sectionrule" />
    </div>
  );
}

const IncomeArrow = () => (
  <svg width="18" height="18" viewBox="0 0 18 18" fill="none" aria-hidden="true">
    <path d="M1.70711 0.292893C1.31658 -0.0976311 0.683418 -0.0976311 0.292893 0.292893C-0.0976311 0.683418 -0.0976311 1.31658 0.292893 1.70711L1.70711 0.292893ZM13 14C13.5523 14 14 13.5523 14 13L14 4C14 3.44771 13.5523 3 13 3C12.4477 3 12 3.44771 12 4L12 12L4 12C3.44771 12 3 12.4477 3 13C3 13.5523 3.44771 14 4 14L13 14ZM0.292893 1.70711L12.2929 13.7071L13.7071 12.2929L1.70711 0.292893L0.292893 1.70711Z" fill="white" />
  </svg>
);
const OutcomeArrow = () => (
  <svg width="18" height="18" viewBox="0 0 18 18" fill="none" aria-hidden="true">
    <path d="M4.03703 12.2929C3.64651 12.6834 3.64651 13.3166 4.03703 13.7071C4.42756 14.0976 5.06072 14.0976 5.45125 13.7071L4.03703 12.2929ZM17.7441 1C17.7441 0.447716 17.2964 4.87118e-07 16.7441 2.34237e-07L7.74414 1.03503e-06C7.19186 6.97852e-07 6.74414 0.447716 6.74414 1C6.74414 1.55229 7.19186 2 7.74414 2L15.7441 2L15.7441 10C15.7441 10.5523 16.1919 11 16.7441 11C17.2964 11 17.7441 10.5523 17.7441 10L17.7441 1ZM5.45125 13.7071L17.4512 1.70711L16.037 0.292894L4.03703 12.2929L5.45125 13.7071Z" fill="white" />
  </svg>
);

function FinancialCard({
  type,
  title,
  value,
  subtitle,
}: {
  type: "income" | "outcome";
  title: string;
  value: string;
  subtitle?: string;
}) {
  return (
    <div className="gpu__fcard">
      <span className="gpu__fcardtitle">{title}</span>
      <div className="gpu__fcardvalue">
        {type === "income" ? <IncomeArrow /> : <OutcomeArrow />}
        <span className="gpu__fcardnum">{value}</span>
      </div>
      {subtitle && <span className="gpu__fcardsub">{subtitle}</span>}
    </div>
  );
}

function EditUpdateModal({
  open,
  loading,
  onCancel,
  onAccept,
}: {
  open: boolean;
  loading: boolean;
  onCancel: () => void;
  onAccept: () => void;
}) {
  if (!open) return null;
  return (
    <div className="gpu__scrim" onClick={onCancel}>
      <div
        className="gpu__modal"
        role="alertdialog"
        aria-modal="true"
        aria-label="Confirm edit"
        onClick={(e) => e.stopPropagation()}
      >
        <button
          type="button"
          className="gpu__modalclose"
          aria-label="Close"
          onClick={onCancel}
        >
          <svg viewBox="0 0 14 14" width="14" height="14" aria-hidden="true">
            <path d="M1 1l12 12M13 1L1 13" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
          </svg>
        </button>
        <h2 className="gpu__modaltitle">Are you sure you want to edit this update?</h2>
        <p className="gpu__modaldesc">
          Saving this update will update any previously published content with
          this new edit.
        </p>
        <div className="gpu__modalactions">
          <button type="button" className="gpu__btn gpu__btn--basic" onClick={onCancel}>
            Cancel
          </button>
          <button
            type="button"
            className={"gpu__btn gpu__btn--primary" + (loading ? " is-loading" : "")}
            disabled={loading}
            onClick={onAccept}
          >
            {loading ? "Saving\u{2026}" : "Save"}
          </button>
        </div>
      </div>
    </div>
  );
}

export default function GvEditProjectUpdate({
  data,
  trackCtx,
  initialStep,
  saveUpdate,
  track,
}: GvEditProjectUpdateProps) {
  const [searchParams] = useSearchParams();
  const urlStep = (searchParams.get("step")?.trim() || initialStep) ?? undefined;
  const stateId = slugToState(urlStep);

  return (
    <GvEditProjectUpdateInner
      key={stateId}
      stateId={stateId}
      data={data}
      trackCtx={trackCtx}
      saveUpdate={saveUpdate}
      track={track}
    />
  );
}

type InnerProps = {
  stateId: ReturnType<typeof slugToState>;
  data: EditUpdateData;
  trackCtx: TrackContext;
  saveUpdate?: SaveFn;
  track?: TrackFn;
};

function GvEditProjectUpdateInner({
  stateId,
  data,
  trackCtx,
  saveUpdate,
  track,
}: InnerProps) {
  const [, setSearchParams] = useSearchParams();
  const [tab, setTab] = useState<GovernanceNavId>("projects");

  const { update, project } = data;

  const draft: EditDraft = useMemo(
    () => ({
      projectId: project.id,
      updateId: update.id,
      health: update.health,
      introduction: update.introduction,
      highlights: update.highlights,
      blockers: update.blockers,
      next_steps: update.next_steps,
      additional_notes: update.additional_notes,
      recordCount: update.financial_records.length,
    }),
    [project.id, update],
  );

  const snapshot = useRef(
    resolveEditSnapshot({ step: stateId, trackCtx, draft, saveUpdate, track }),
  ).current;

  const [state, send] = useMachine(editUpdateMachine, {
    input: { trackCtx, draft, saveUpdate, track },
    snapshot,
  });

  const value = state.value as string;
  const step = stateToSlug(value);

  const [health, setHealth] = useState<ProjectHealth>(update.health);
  const [fields, setFields] = useState<Record<string, string>>({
    introduction: update.introduction,
    highlights: update.highlights,
    blockers: update.blockers,
    next_steps: update.next_steps,
    additional_notes: update.additional_notes,
  });
  const setField = (id: string, v: string) =>
    setFields((f) => ({ ...f, [id]: v }));

  const [csv, setCsv] = useState<string>(() =>
    recordsToCsv(update.financial_records),
  );

  const disclosed = useMemo(
    () => update.financial_records.reduce((s, r) => s + (Number(r.amount) || 0), 0),
    [update.financial_records],
  );

  const lastStep = useRef<string | null>(null);
  useEffect(() => {
    if (lastStep.current === step) return;
    lastStep.current = step;
    setSearchParams(
      (prev) => {
        const params = new URLSearchParams(prev);
        if (params.get("step") === step) return params;
        params.set("step", step);
        return params;
      },
      { replace: true, preventScrollReset: true },
    );
  }, [step, setSearchParams]);

  const confirmOpen = value === "confirm" || value === "saving" || value === "saveError";
  const saving = value === "saving";

  return (
    <GovernanceChrome active={tab} onTab={setTab}>
      <div className="gpu" data-step={step} data-source={data.source}>
        <div className="gpu__inner">
          <header className="gpu__header">
            <h1 className="gpu__h1">Edit Grant Update</h1>
            <p className="gpu__sub">
              Editing the latest published update for{" "}
              <strong>{project.title}</strong>. Update the progress, raise any
              blockers, and disclose how the grant funds were used. Saving will
              overwrite the previously published content.
            </p>
          </header>

          {value === "general" && (
            <section className="gpu__section">
              <SectionHead n={1} title="General" validated />
              <div className="gpu__sectionbody">
                <div className="gpu__field">
                  <label className="gpu__label">Project Health</label>
                  <div className="gpu__healthrow">
                    {HEALTHS.map((h) => (
                      <button
                        key={h.id}
                        type="button"
                        className={"gpu__health" + (health === h.id ? " " + h.cls : "")}
                        aria-pressed={health === h.id}
                        onClick={() => setHealth(h.id)}
                      >
                        {h.label}
                      </button>
                    ))}
                  </div>
                </div>

                {GENERAL_FIELDS.map((f) => {
                  const v = fields[f.id] ?? "";
                  return (
                    <div className="gpu__field" key={f.id}>
                      <label className="gpu__label">{f.label}</label>
                      <div className="gpu__editor">
                        <div className="gpu__editortoolbar">
                          <span className="gpu__tbicon" title="Bold">B</span>
                          <span className="gpu__tbicon gpu__tbicon--italic" title="Italic">I</span>
                          <span className="gpu__tbicon" title="Link">&#x1F517;</span>
                          <span className="gpu__tbicon" title="List">&#x2263;</span>
                          <span className="gpu__tbspacer" />
                          <span className="gpu__tbtext">Preview</span>
                        </div>
                        <textarea
                          className="gpu__textarea"
                          value={v}
                          onChange={(e) => setField(f.id, e.target.value)}
                          style={{ minHeight: f.minHeight }}
                        />
                      </div>
                      <div className="gpu__message">
                        ({v.length} out of {f.max} characters)
                      </div>
                    </div>
                  );
                })}
              </div>

              <div className="gpu__actions">
                <button
                  type="button"
                  className="gpu__btn gpu__btn--primary"
                  onClick={() => send({ type: "NEXT" })}
                >
                  Continue to financials
                </button>
                <button type="button" className="gpu__btn gpu__btn--basic">
                  Preview update
                </button>
              </div>
            </section>
          )}

          {value === "financials" && (
            <section className="gpu__section">
              <SectionHead n={2} title="Financials" isNew validated />
              <div className="gpu__sectionbody">
                <div className="gpu__field">
                  <div className="gpu__fcards">
                    <FinancialCard
                      type="income"
                      title="Funds released since last update"
                      value={CURRENCY(data.fundsReleasedSinceLastUpdate)}
                      subtitle={`${data.fundsReleasedTxCount} tx. last one made ${data.fundsReleasedLastTxDate}`}
                    />
                    <FinancialCard
                      type="outcome"
                      title="Funds disclosed this update"
                      value={CURRENCY(disclosed)}
                      subtitle={`${CURRENCY(
                        Math.max(0, data.fundsReleasedSinceLastUpdate - disclosed),
                      )} left undisclosed`}
                    />
                  </div>
                </div>

                <div className="gpu__field">
                  <label className="gpu__label">Reporting</label>
                  <p className="gpu__reportsub">
                    Bring some light onto how this project is utilizing funds, CSV
                    syntax.
                  </p>
                  <div className="gpu__csv">
                    <div className="gpu__csvgutter" aria-hidden="true">
                      {csv.split("\n").map((_, i) => (
                        <span key={i}>{i}</span>
                      ))}
                    </div>
                    <textarea
                      className="gpu__csvinput"
                      value={csv}
                      onChange={(e) => setCsv(e.target.value)}
                      spellCheck={false}
                    />
                  </div>
                  <div className="gpu__dropzone">
                    <svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                      <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
                      <path d="M14 2v6h6" />
                    </svg>
                    <span>
                      Drag and drop a CSV file or{" "}
                      <span className="gpu__droplink">choose a file</span>.
                    </span>
                  </div>
                </div>

                <div className="gpu__field">
                  <label className="gpu__label">Summary</label>
                  <div className="gpu__summary">
                    <div className="gpu__summaryhead">
                      <span>Category</span>
                      <span>Description</span>
                      <span>Token</span>
                      <span className="gpu__num">Amount</span>
                    </div>
                    {update.financial_records.map((r, i) => (
                      <div className="gpu__summaryrow" key={i}>
                        <span className="gpu__cat">{r.category}</span>
                        <span className="gpu__desc">{r.description}</span>
                        <span className="gpu__token">{r.token}</span>
                        <span className="gpu__num">{CURRENCY(r.amount)}</span>
                      </div>
                    ))}
                    <div className="gpu__summarytotal">
                      <span>Total disclosed</span>
                      <span className="gpu__num">{CURRENCY(disclosed)}</span>
                    </div>
                  </div>
                </div>
              </div>

              <div className="gpu__actions">
                <button
                  type="button"
                  className="gpu__btn gpu__btn--primary"
                  onClick={() => send({ type: "REVIEW" })}
                >
                  Save update
                </button>
                <button
                  type="button"
                  className="gpu__btn gpu__btn--basic"
                  onClick={() => send({ type: "BACK" })}
                >
                  Back
                </button>
              </div>
            </section>
          )}

          {value === "done" && (
            <section className="gpu__section">
              <SectionHead n={1} title="Update saved" validated />
              <div className="gpu__sectionbody">
                <p className="gpu__sub">
                  Your edit to this Project Update has been saved. (This save is{" "}
                  <strong>simulated</strong> &#x2014; governance has no write API here; the
                  flow, states, and metrics are real.)
                </p>
                <div className="gpu__fcards">
                  <FinancialCard
                    type="income"
                    title="Health"
                    value={
                      HEALTHS.find((h) => h.id === health)?.label ?? health
                    }
                  />
                  <FinancialCard
                    type="outcome"
                    title="Funds disclosed"
                    value={CURRENCY(disclosed)}
                    subtitle={`${update.financial_records.length} records`}
                  />
                </div>
              </div>
            </section>
          )}
        </div>
      </div>

      <EditUpdateModal
        open={confirmOpen}
        loading={saving}
        onCancel={() => send({ type: "CANCEL" })}
        onAccept={() => send({ type: "SAVE" })}
      />
      {value === "saveError" && (
        <div className="gpu__scrim">
          <div className="gpu__modal" role="alertdialog" aria-modal="true">
            <h2 className="gpu__modaltitle">Couldn't save your update</h2>
            <p className="gpu__modaldesc" role="alert">
              {state.context.error ?? "Save failed."}
            </p>
            <div className="gpu__modalactions">
              <button
                type="button"
                className="gpu__btn gpu__btn--basic"
                onClick={() => send({ type: "CANCEL" })}
              >
                Back
              </button>
              <button
                type="button"
                className="gpu__btn gpu__btn--primary"
                onClick={() => send({ type: "RETRY" })}
              >
                Retry
              </button>
            </div>
          </div>
        </div>
      )}
    </GovernanceChrome>
  );
}
