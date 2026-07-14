import { useEffect, useRef, useState } from "react";
import { useMachine } from "@xstate/react";
import { useSearchParams } from "react-router";

import GovernanceChrome from "@ui/governance/frames/GovernanceChrome";
import GvProposalDetailSuccessOutcomeScreens from "@ui/governance/components/GvProposalDetailSuccessOutcomeScreens";
import "@ui/governance/pages/gvsubmittender.css";

import type { TrackContext } from "@core/lib/telemetry/track";
import {
  getQuarters,
  validateTenderDetails,
  isEthAddress,
  type Pitch,
  type SubmitTenderData,
  type TenderForm,
} from "@data/lib/catalyst/governance/submit-tender";
import {
  tenderMachine,
  resolveTenderSnapshot,
  slugToState,
  stateToSlug,
  type SubmitFn,
  type TenderSeed,
  type TrackFn,
} from "./machine";

export type SubmitTenderWizardProps = {
  trackCtx: TrackContext;
  data: SubmitTenderData;
  pitch: Pitch | null;
  votingPower: number;
  initialStep?: string;
  submit?: SubmitFn;
  track?: TrackFn;
};

export default function SubmitTenderWizard(props: SubmitTenderWizardProps) {
  const [searchParams] = useSearchParams();

  const urlStep = (searchParams.get("step")?.trim() || props.initialStep) ?? undefined;
  const stateId = slugToState(urlStep);

  return (
    <SubmitTenderWizardInner
      key={`${stateId}:${props.votingPower}`}
      stateId={stateId}
      {...props}
    />
  );
}

type InnerProps = SubmitTenderWizardProps & {
  stateId: ReturnType<typeof slugToState>;
};

function SubmitTenderWizardInner({
  stateId,
  trackCtx,
  data,
  pitch,
  votingPower,
  submit,
  track,
}: InnerProps) {
  const [, setSearchParams] = useSearchParams();

  const seed: TenderSeed = {
    votingPower,
    threshold: data.submission_threshold_tender,
    linkedProposalId: pitch?.id ?? data.default_linked_proposal_id ?? "",
  };

  const snapshot = useRef(
    resolveTenderSnapshot({ step: stateId, trackCtx, seed, submit, track }),
  ).current;

  const [state, send] = useMachine(tenderMachine, {
    input: { trackCtx, seed, submit, track },
    snapshot,
  });

  const value = state.value as string;
  const step = stateToSlug(value);
  const form = state.context.form;

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

  const setForm = (patch: Partial<TenderForm>) => send({ type: "SET_FORM", patch });
  const vpNotMet = votingPower < data.submission_threshold_tender;

  return (
    <GovernanceChrome active="proposals" account={vpNotMet ? "Sign in" : "0x9f3c\u{2026}7a21"} onTab={() => {}}>
      <div className="gvst" data-step={step}>
        <header className="gvst__header">
          <h1 className="gvst__title">{data.page.title}</h1>
        </header>
        <div className="gvst__intro">
          <p>{data.page.description}</p>
        </div>

        {value === "parent" && (
          <ParentStep
            data={data}
            pitch={pitch}
            votingPower={votingPower}
            vpNotMet={vpNotMet}
            onStart={() => send({ type: "START" })}
            onGate={() => send({ type: "GATE" })}
          />
        )}

        {value === "gated" && (
          <GatedStep data={data} votingPower={votingPower} onRetry={() => send({ type: "RETRY" })} />
        )}

        {value === "details" && (
          <DetailsStep
            data={data}
            form={form}
            onChange={setForm}
            onBack={() => send({ type: "BACK" })}
            onNext={() => send({ type: "NEXT" })}
          />
        )}

        {value === "coauthors" && (
          <CoauthorsStep
            data={data}
            form={form}
            onChange={setForm}
            onBack={() => send({ type: "BACK" })}
            onNext={() => send({ type: "NEXT" })}
          />
        )}

        {value === "review" && (
          <ReviewStep
            data={data}
            pitch={pitch}
            form={form}
            onBack={() => send({ type: "BACK" })}
            onSubmit={() => send({ type: "SUBMIT" })}
          />
        )}

        {value === "submitting" && <SubmittingStep />}

        {value === "error" && (
          <ErrorStep
            data={data}
            error={state.context.error}
            onRetry={() => send({ type: "RETRY" })}
            onBack={() => send({ type: "BACK" })}
          />
        )}
      </div>

      {value === "success" && (
        <GvProposalDetailSuccessOutcomeScreens variant="pending" />
      )}
    </GovernanceChrome>
  );
}

function PitchCard({ pitch }: { pitch: Pitch | null }) {
  if (!pitch) {
    return (
      <p className="gvst__sublabel">
        No linked Pitch is available. A Tender refines a passed Pitch &#x2014; start from a
        Pitch proposal to preselect it here.
      </p>
    );
  }
  return (
    <div className="gvst__selectwrap">
      <select className="gvst__select" value={pitch.id} disabled aria-readonly="true">
        <option value={pitch.id}>Pitch: {pitch.title}</option>
      </select>
    </div>
  );
}

function ParentStep({
  data,
  pitch,
  votingPower,
  vpNotMet,
  onStart,
  onGate,
}: {
  data: SubmitTenderData;
  pitch: Pitch | null;
  votingPower: number;
  vpNotMet: boolean;
  onStart: () => void;
  onGate: () => void;
}) {
  return (
    <div className="gvst__form">
      <div className="gvst__section is-disabled">
        <div className="gvst__label">
          <span>{data.page.linked_proposal_label}</span>
        </div>
        <PitchCard pitch={pitch} />
      </div>

      <div className="gvst__section">
        <p className="gvst__sublabel">
          Voting Power: <strong>{votingPower.toLocaleString()}</strong> VP &#x2014; this
          action requires at least {data.submission_threshold_tender.toLocaleString()}{" "}
          VP.
        </p>
      </div>

      {vpNotMet ? (
        <>
          <div className="gvst__section">
            <p className="gvst__vpnotice">
              {data.errors.submission_vp_not_met ??
                `This action requires at least ${data.submission_threshold_tender} VP.`}
            </p>
          </div>
          <div className="gvst__submitrow">
            <button type="button" className="gvst__submit" onClick={onGate}>
              Why can't I submit?
            </button>
          </div>
        </>
      ) : (
        <div className="gvst__submitrow">
          <button type="button" className="gvst__submit" onClick={onStart} disabled={!pitch}>
            Start Tender
          </button>
        </div>
      )}
    </div>
  );
}

function GatedStep({
  data,
  votingPower,
  onRetry,
}: {
  data: SubmitTenderData;
  votingPower: number;
  onRetry: () => void;
}) {
  return (
    <div className="gvst__form" role="alert">
      <div className="gvst__section">
        <p className="gvst__vpnotice">
          {data.errors.submission_vp_not_met ??
            `This action requires at least ${data.submission_threshold_tender} VP.`}
        </p>
        <p className="gvst__sublabel">
          Your Voting Power is {votingPower.toLocaleString()} VP.{" "}
          <a href="https://account.decentraland.org/">Buy MANA</a> to get VP, or{" "}
          <a href="https://forum.decentraland.org/t/open-call-for-delegates-apply-now/5840/5">
            run for delegate
          </a>
          .
        </p>
      </div>
      <div className="gvst__submitrow">
        <button type="button" className="gvst__submit" onClick={onRetry}>
          Back
        </button>
      </div>
    </div>
  );
}

const TEXT_SECTIONS: Array<{ name: keyof TenderForm; key: string }> = [
  { name: "summary", key: "summary" },
  { name: "problem_statement", key: "problem_statement" },
  { name: "technical_specification", key: "technical_specification" },
  { name: "use_cases", key: "use_cases" },
  { name: "deliverables", key: "deliverables" },
];

function DetailsStep({
  data,
  form,
  onChange,
  onBack,
  onNext,
}: {
  data: SubmitTenderData;
  form: TenderForm;
  onChange: (patch: Partial<TenderForm>) => void;
  onBack: () => void;
  onNext: () => void;
}) {
  const quarters = getQuarters();
  const invalid = validateTenderDetails(form);

  return (
    <form
      className="gvst__form"
      onSubmit={(e) => {
        e.preventDefault();
        if (!invalid) onNext();
      }}
    >
      <div className="gvst__section">
        <div className="gvst__label">
          <span>{data.fields.project_name.label}</span>
        </div>
        <input
          className="gvst__input"
          type="text"
          value={form.project_name}
          maxLength={data.fields.project_name.max_length ?? undefined}
          onChange={(e) => onChange({ project_name: e.target.value })}
        />
        <div className="gvst__message">
          <span>
            ({form.project_name.length} out of {data.fields.project_name.max_length} characters)
          </span>
        </div>
      </div>

      {TEXT_SECTIONS.map(({ name }) => {
        const field = data.fields[name as keyof SubmitTenderData["fields"]];
        const v = String(form[name] ?? "");
        return (
          <div key={name} className="gvst__section">
            <div className="gvst__label">
              <span>{field.label}</span>
              <sup className="gvst__notice">(markdown)</sup>
            </div>
            {field.sublabel ? <p className="gvst__sublabel">{field.sublabel}</p> : null}
            <div className="gvst__mdfield">
              <textarea
                className="gvst__textarea"
                name={name}
                value={v}
                onChange={(e) => onChange({ [name]: e.target.value } as Partial<TenderForm>)}
              />
            </div>
            <div className="gvst__message">
              <span>
                ({v.length} out of {field.max_length} characters)
              </span>
            </div>
          </div>
        );
      })}

      <div className="gvst__section">
        <div className="gvst__label">
          <span>{data.fields.target_release_quarter.label}</span>
        </div>
        <p className="gvst__sublabel">{data.fields.target_release_quarter.sublabel}</p>
        <div className="gvst__selectwrap">
          <select
            className="gvst__select"
            value={form.target_release_quarter}
            onChange={(e) => onChange({ target_release_quarter: e.target.value })}
          >
            <option value="" disabled>
              {data.fields.target_release_quarter.placeholder ?? "Select a target release quarter"}
            </option>
            {quarters.map((q) => (
              <option key={q} value={q}>
                {q}
              </option>
            ))}
          </select>
        </div>
      </div>

      <div className="gvst__submitrow">
        <button type="button" className="gvst__submit" onClick={onBack}>
          Back
        </button>
        <button type="submit" className="gvst__submit" disabled={!!invalid}>
          Next: co-authors
        </button>
      </div>
      {invalid ? (
        <div className="gvst__section">
          <p className="gvst__vpnotice">{invalid.message}</p>
        </div>
      ) : null}
    </form>
  );
}

function CoauthorsStep({
  data,
  form,
  onChange,
  onBack,
  onNext,
}: {
  data: SubmitTenderData;
  form: TenderForm;
  onChange: (patch: Partial<TenderForm>) => void;
  onBack: () => void;
  onNext: () => void;
}) {
  const [draft, setDraft] = useState("");
  const max = data.fields.coAuthors.max ?? 5;
  const canAdd =
    form.coAuthors.length < max && isEthAddress(draft) && !form.coAuthors.includes(draft.trim());

  const add = () => {
    if (!canAdd) return;
    onChange({ coAuthors: [...form.coAuthors, draft.trim()] });
    setDraft("");
  };
  const remove = (i: number) =>
    onChange({ coAuthors: form.coAuthors.filter((_, idx) => idx !== i) });

  return (
    <div className="gvst__form">
      <div className="gvst__section">
        <div className="gvst__label">
          <span>{data.fields.coAuthors.label}</span>
          <sup className="gvst__optional">(optional)</sup>
        </div>
        {data.fields.coAuthors.help ? (
          <p className="gvst__sublabel">{data.fields.coAuthors.help}</p>
        ) : null}

        <div className="gvst__coauthors">
          {form.coAuthors.map((c, i) => (
            <span className="gvst__coauthor" key={c}>
              {c}
              <button
                type="button"
                className="gvst__coremove"
                aria-label="Remove co-author"
                onClick={() => remove(i)}
              >
                &#xD7;
              </button>
            </span>
          ))}
        </div>

        {form.coAuthors.length < max ? (
          <div className="gvst__selectwrap">
            <input
              className="gvst__input"
              type="text"
              placeholder={"0x\u{2026} co-author address"}
              aria-label="Co-author address"
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
            />
            <button type="button" className="gvst__coadd" onClick={add} disabled={!canAdd}>
              + Add co-author
            </button>
          </div>
        ) : null}
      </div>

      <div className="gvst__submitrow">
        <button type="button" className="gvst__submit" onClick={onBack}>
          Back
        </button>
        <button type="button" className="gvst__submit" onClick={onNext}>
          Next: review
        </button>
      </div>
    </div>
  );
}

function ReviewRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="gvst__section">
      <div className="gvst__label">
        <span>{label}</span>
      </div>
      <p className="gvst__sublabel" style={{ whiteSpace: "pre-wrap" }}>
        {value || "\u{2014}"}
      </p>
    </div>
  );
}

function ReviewStep({
  data,
  pitch,
  form,
  onBack,
  onSubmit,
}: {
  data: SubmitTenderData;
  pitch: Pitch | null;
  form: TenderForm;
  onBack: () => void;
  onSubmit: () => void;
}) {
  return (
    <div className="gvst__form">
      <ReviewRow label={data.page.linked_proposal_label} value={pitch ? `Pitch: ${pitch.title}` : form.linked_proposal_id} />
      <ReviewRow label={data.fields.project_name.label} value={form.project_name} />
      <ReviewRow label={data.fields.summary.label} value={form.summary} />
      <ReviewRow label={data.fields.problem_statement.label} value={form.problem_statement} />
      <ReviewRow label={data.fields.technical_specification.label} value={form.technical_specification} />
      <ReviewRow label={data.fields.use_cases.label} value={form.use_cases} />
      <ReviewRow label={data.fields.deliverables.label} value={form.deliverables} />
      <ReviewRow label={data.fields.target_release_quarter.label} value={form.target_release_quarter} />
      <ReviewRow
        label={data.fields.coAuthors.label}
        value={form.coAuthors.length ? form.coAuthors.join(", ") : "None"}
      />

      <div className="gvst__submitrow">
        <button type="button" className="gvst__submit" onClick={onBack}>
          Back
        </button>
        <button type="button" className="gvst__submit" onClick={onSubmit}>
          {data.page.submit_label}
        </button>
      </div>
    </div>
  );
}

function SubmittingStep() {
  return (
    <div className="gvst__form" aria-busy="true">
      <div className="gvst__loading">
        <span className="gvst__spinner" aria-hidden="true" />
        Submitting your Tender proposal to Snapshot&#x2026;
      </div>
    </div>
  );
}

function ErrorStep({
  data,
  error,
  onRetry,
  onBack,
}: {
  data: SubmitTenderData;
  error?: string;
  onRetry: () => void;
  onBack: () => void;
}) {
  return (
    <div className="gvst__form">
      <div className="gvst__section">
        <div className="gvst__errorbox" role="alert">
          <span className="gvst__errorlabel">There was an error.</span>
          <span className="gvst__errortext">{error || data.submit_error}</span>
        </div>
      </div>
      <div className="gvst__submitrow">
        <button type="button" className="gvst__submit" onClick={onBack}>
          Back
        </button>
        <button type="button" className="gvst__submit" onClick={onRetry}>
          Retry
        </button>
      </div>
    </div>
  );
}
