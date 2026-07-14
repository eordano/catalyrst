import { useEffect, useRef, useState, type ChangeEvent } from "react";
import { useMachine } from "@xstate/react";
import { useSearchParams } from "react-router";

import SitesChrome from "@ui/web/frames/SitesChrome";
import StReportSuccess from "@ui/web/pages/StReportSuccess";
import { asset } from "@ui/asset";
import "@ui/web/pages/streport.css";

import type { TrackContext } from "@core/lib/telemetry/track";
import {
  REPORT_LIMITS,
  type EvidenceUpload,
  type ReasonOption,
  type ReportDraft,
  type ReportReason,
  type SubmitReportFn,
} from "@data/lib/catalyst/landings/report";
import {
  reportMachine,
  resolveReportSnapshot,
  slugToState,
  stateToSlug,
  type ReportStateId,
  type TrackFn,
} from "./machine";

const DEFAULT_REASONS: ReasonOption[] = [
  { value: "scam_phishing", label: "Scam/Phishing" },
  { value: "illegal_content", label: "Illegal Content" },
  { value: "harassment", label: "Harassment" },
  { value: "cheating", label: "Cheating" },
  { value: "impersonation", label: "Impersonation" },
];

const SAMPLE_SEED: Partial<ReportDraft> = {
  reportedAddress: "0x6d7e3b6e5e3c3f3d4f5a6b7c8d9e0f1a2b3c4d5e",
  reason: "harassment",
  description:
    "Repeatedly followed my avatar to spam abusive messages in chat at Genesis Plaza.",
  evidence: [
    { id: "evt-1", name: "chat-log-screenshot.png", size: 248213 },
    { id: "evt-2", name: "clip-2026-06-23.mp4", size: 4821330 },
  ],
};

export type ReportWizardProps = {
  trackCtx: TrackContext;
  playerAddress?: string;
  reasonOptions?: ReasonOption[];
  initialStep?: string;
  submit?: SubmitReportFn;
  track?: TrackFn;
};

export default function ReportWizard({
  trackCtx,
  playerAddress,
  reasonOptions,
  initialStep,
  submit,
  track,
}: ReportWizardProps) {
  const [searchParams] = useSearchParams();

  const urlStep = (searchParams.get("step")?.trim() || initialStep) ?? undefined;
  const stateId = slugToState(urlStep);

  return (
    <ReportWizardInner
      key={stateId}
      stateId={stateId}
      trackCtx={trackCtx}
      playerAddress={playerAddress}
      reasonOptions={reasonOptions ?? DEFAULT_REASONS}
      submit={submit}
      track={track}
    />
  );
}

type InnerProps = {
  stateId: ReportStateId;
  trackCtx: TrackContext;
  playerAddress?: string;
  reasonOptions: ReasonOption[];
  submit?: SubmitReportFn;
  track?: TrackFn;
};

function ReportWizardInner({
  stateId,
  trackCtx,
  playerAddress,
  reasonOptions,
  submit,
  track,
}: InnerProps) {
  const [, setSearchParams] = useSearchParams();

  const snapshot = useRef(
    resolveReportSnapshot({
      step: stateId,
      trackCtx,
      playerAddress,
      seed: stateId === "intro" || stateId === "target" ? undefined : SAMPLE_SEED,
      submit,
      track,
    }),
  ).current;

  const [state, send] = useMachine(reportMachine, {
    input: { trackCtx, playerAddress, submit, track },
    snapshot,
  });

  const value = state.value as string;
  const step = stateToSlug(value);
  const draft = state.context.draft;

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

  if (value === "success") {
    return <StReportSuccess />;
  }

  return (
    <SitesChrome active="legal" overlayNav>
      <div className="streport" data-step={step}>
        <div className="streport__bg" aria-hidden="true" />
        <div className="streport__content">
          <div className="streport__card">
            <div className="streport__logowrap">
              <DclLogoMark />
              <h1 className="streport__title">Report User</h1>
            </div>

            <StepIndicator current={value} />

            {value === "intro" && (
              <IntroStep
                signedIn={Boolean(playerAddress)}
                onStart={() => send({ type: "START" })}
              />
            )}

            {value === "target" && (
              <TargetStep
                playerAddress={draft.playerAddress}
                initial={draft.reportedAddress}
                onBack={() => send({ type: "BACK" })}
                onContinue={(reportedAddress) =>
                  send({ type: "SET_TARGET", reportedAddress })
                }
              />
            )}

            {value === "category" && (
              <CategoryStep
                options={reasonOptions}
                initial={draft.reason}
                onBack={() => send({ type: "BACK" })}
                onContinue={(reason) => send({ type: "SET_CATEGORY", reason })}
              />
            )}

            {value === "details" && (
              <DetailsStep
                initial={draft.description}
                onBack={() => send({ type: "BACK" })}
                onContinue={(description) =>
                  send({ type: "SET_DETAILS", description })
                }
              />
            )}

            {value === "evidence" && (
              <EvidenceStep
                evidence={draft.evidence}
                onChange={(evidence) => send({ type: "SET_EVIDENCE", evidence })}
                onBack={() => send({ type: "BACK" })}
                onContinue={() => send({ type: "CONTINUE" })}
              />
            )}

            {value === "review" && (
              <ReviewStep
                draft={draft}
                reasonOptions={reasonOptions}
                onConfirm={(confirmAccuracy) =>
                  send({ type: "SET_CONFIRM", confirmAccuracy })
                }
                onBack={() => send({ type: "BACK" })}
                onSubmit={() => send({ type: "SUBMIT" })}
              />
            )}

            {value === "submitting" && (
              <SubmittingStep />
            )}

            {value === "error" && (
              <ErrorStep
                message={state.context.error}
                onRetry={() => send({ type: "RETRY" })}
              />
            )}
          </div>

          <p className="streport__footer">
            Need help instead?
            <br />
            For urgent issues or security concerns, please{" "}
            <a
              className="streport__footerlink"
              href="https://decentraland.org/help"
              target="_blank"
              rel="noopener noreferrer"
            >
              contact the Support team
            </a>
            .
          </p>
        </div>
      </div>
    </SitesChrome>
  );
}

const STEP_ORDER: { id: string; label: string }[] = [
  { id: "intro", label: "Start" },
  { id: "target", label: "Target" },
  { id: "category", label: "Reason" },
  { id: "details", label: "Details" },
  { id: "evidence", label: "Evidence" },
  { id: "review", label: "Review" },
];

function StepIndicator({ current }: { current: string }) {
  const effective =
    current === "submitting" || current === "error" ? "review" : current;
  const idx = STEP_ORDER.findIndex((s) => s.id === effective);
  return (
    <div className="streport__helper" role="status" aria-live="polite">
      {STEP_ORDER.map((s, i) => (
        <span key={s.id} style={{ opacity: i <= idx ? 1 : 0.45 }}>
          {i > 0 ? " \u{203A} " : ""}
          {s.label}
        </span>
      ))}
    </div>
  );
}

function NumberBullet({ number }: { number: number }) {
  return <span className="streport__bullet">{number}</span>;
}

function DclLogoMark() {
  return (
    <img
      className="streport__logo"
      src={asset("assets/dcl-logo.png")}
      alt="Decentraland"
      width={64}
      height={64}
      onError={(e) => {
        (e.currentTarget as HTMLImageElement).style.display = "none";
      }}
    />
  );
}

type WizardControlsProps = {
  onBack?: () => void;
  backLabel?: string;
  primaryLabel: string;
  primaryDisabled?: boolean;
  onPrimary: () => void;
};

function WizardControls({
  onBack,
  backLabel = "Back",
  primaryLabel,
  primaryDisabled,
  onPrimary,
}: WizardControlsProps) {
  return (
    <div
      className="streport__field"
      style={{ flexDirection: "row", gap: 12, justifyContent: "space-between" }}
    >
      {onBack ? (
        <button type="button" className="streport__addfile" onClick={onBack}>
          {backLabel}
        </button>
      ) : (
        <span />
      )}
      <button
        type="button"
        className="streport__submit"
        onClick={onPrimary}
        disabled={primaryDisabled}
        style={{ margin: 0, width: "auto", minWidth: 180 }}
      >
        {primaryLabel}
      </button>
    </div>
  );
}

function IntroStep({
  signedIn,
  onStart,
}: {
  signedIn: boolean;
  onStart: () => void;
}) {
  return (
    <>
      <div className="streport__helper">
        Report a player for abuse &#x2014; scam/phishing, illegal content, harassment,
        cheating, or impersonation. Reports are tied to your wallet to prevent
        abuse; your identity will not be publicly shared. You will provide the
        reported wallet, a reason, a description, and evidence, then confirm and
        submit.
      </div>
      {!signedIn && (
        <div className="streport__signin">
          <span>Please sign in with your wallet to submit a report.</span>
          <button type="button" className="streport__signinbtn">
            Sign In
          </button>
        </div>
      )}
      <WizardControls primaryLabel="Start a report" onPrimary={onStart} />
    </>
  );
}

function TargetStep({
  playerAddress,
  initial,
  onBack,
  onContinue,
}: {
  playerAddress: string;
  initial: string;
  onBack: () => void;
  onContinue: (reportedAddress: string) => void;
}) {
  const [reported, setReported] = useState(initial);
  const [touched, setTouched] = useState(false);
  const valid = /^0x[a-fA-F0-9]{40}$/.test(reported.trim());
  const showError = touched && !valid;

  return (
    <>
      <div className="streport__field">
        <div className="streport__label">
          <NumberBullet number={1} />
          Your Wallet Address
          <span className="streport__required">*</span>
        </div>
        <div className="streport__helper">
          Reports are tied to your wallet to prevent abuse. Your identity will
          not be publicly shared.
        </div>
        <input
          className="streport__input"
          placeholder="Sign in to set this"
          value={playerAddress}
          disabled
          readOnly
        />
      </div>

      <div className="streport__field">
        <div className="streport__label">
          <NumberBullet number={2} />
          Reported User Wallet
          <span className="streport__required">*</span>
        </div>
        <div className="streport__helper">
          This is the wallet address of the user you are reporting.
        </div>
        <input
          className={"streport__input" + (showError ? " is-error" : "")}
          placeholder="Write or paste an address here..."
          value={reported}
          onChange={(e) => setReported(e.target.value)}
        />
        {showError && (
          <div className="streport__fielderror">
            Please enter a valid wallet address
          </div>
        )}
      </div>

      <WizardControls
        onBack={onBack}
        primaryLabel="Continue"
        onPrimary={() => {
          setTouched(true);
          onContinue(reported);
        }}
      />
    </>
  );
}

function CategoryStep({
  options,
  initial,
  onBack,
  onContinue,
}: {
  options: ReasonOption[];
  initial: ReportReason | "";
  onBack: () => void;
  onContinue: (reason: ReportReason) => void;
}) {
  const [reason, setReason] = useState<string>(initial);
  const [open, setOpen] = useState(false);
  const selected = options.find((o) => o.value === reason);

  return (
    <>
      <div className="streport__field">
        <div className="streport__label">
          <NumberBullet number={3} />
          Reason for Report
          <span className="streport__required">*</span>
        </div>
        <div className="streport__helper">
          Select the option that best describes the issue.
        </div>
        <div className="streport__selectwrap">
          <button
            type="button"
            className={"streport__select" + (open ? " is-open" : "")}
            onClick={() => setOpen((v) => !v)}
            aria-haspopup="listbox"
            aria-expanded={open}
          >
            <span className={selected ? "" : "streport__selectplaceholder"}>
              {selected ? selected.label : "Choose a reason"}
            </span>
          </button>
          {open && (
            <ul className="streport__menu" role="listbox">
              {options.map((opt) => (
                <li
                  key={opt.value}
                  role="option"
                  aria-selected={reason === opt.value}
                  className={
                    "streport__option" + (reason === opt.value ? " is-selected" : "")
                  }
                  onClick={() => {
                    setReason(opt.value);
                    setOpen(false);
                  }}
                >
                  {opt.label}
                </li>
              ))}
            </ul>
          )}
        </div>
      </div>

      <WizardControls
        onBack={onBack}
        primaryLabel="Continue"
        primaryDisabled={!selected}
        onPrimary={() => selected && onContinue(selected.value as ReportReason)}
      />
    </>
  );
}

function DetailsStep({
  initial,
  onBack,
  onContinue,
}: {
  initial: string;
  onBack: () => void;
  onContinue: (description: string) => void;
}) {
  const [description, setDescription] = useState(initial);
  const [touched, setTouched] = useState(false);
  const valid = description.trim().length > 0;
  const showError = touched && !valid;

  return (
    <>
      <div className="streport__field">
        <div className="streport__label">
          <NumberBullet number={4} />
          Description
          <span className="streport__required">*</span>
        </div>
        <div className="streport__helper">
          Describe what happened. Include when and where the incident occurred if
          possible.
        </div>
        <div className="streport__inputgroup">
          <textarea
            className="streport__textarea"
            rows={4}
            placeholder="Write your description here..."
            value={description}
            onChange={(e) =>
              setDescription(e.target.value.slice(0, REPORT_LIMITS.descriptionMax))
            }
          />
          <div className="streport__counter">
            {description.length} / {REPORT_LIMITS.descriptionMax}
          </div>
          <div className="streport__hint">
            The more detail you provide, the easier it is for moderators to review
            your report.
          </div>
        </div>
        {showError && (
          <div className="streport__fielderror">
            Please include a description of your report
          </div>
        )}
      </div>

      <WizardControls
        onBack={onBack}
        primaryLabel="Continue"
        onPrimary={() => {
          setTouched(true);
          onContinue(description);
        }}
      />
    </>
  );
}

function EvidenceStep({
  evidence,
  onChange,
  onBack,
  onContinue,
}: {
  evidence: EvidenceUpload[];
  onChange: (evidence: EvidenceUpload[]) => void;
  onBack: () => void;
  onContinue: () => void;
}) {
  const inputRef = useRef<HTMLInputElement>(null);
  const [touched, setTouched] = useState(false);

  function handleSelect(e: ChangeEvent<HTMLInputElement>) {
    const selected = Array.from(e.target.files ?? []);
    const remaining = REPORT_LIMITS.maxFiles - evidence.length;
    const next: EvidenceUpload[] = selected
      .slice(0, remaining)
      .map((file, i) => ({
        id: `${Date.now()}-${i}`,
        name: file.name,
        size: file.size,
        blob: file,
        contentType: file.type,
      }));
    if (next.length > 0) onChange([...evidence, ...next]);
    if (inputRef.current) inputRef.current.value = "";
  }

  function remove(id: string) {
    onChange(evidence.filter((f) => f.id !== id));
  }

  const showError = touched && evidence.length === 0;

  return (
    <>
      <div className="streport__field">
        <div className="streport__label">
          <NumberBullet number={5} />
          Evidence
          <span className="streport__required">*</span>
        </div>
        <div className="streport__helper">
          Upload up to {REPORT_LIMITS.maxFiles} screenshots, videos, or links to
          support your report.
        </div>
        <div className="streport__fileupload">
          <input
            ref={inputRef}
            type="file"
            multiple
            className="streport__fileinput"
            accept="image/png,image/jpeg,image/gif,image/webp,video/mp4,video/webm,application/pdf"
            onChange={handleSelect}
          />
          {evidence.length > 0 && (
            <div className="streport__chips">
              {evidence.map((file) => (
                <span key={file.id} className="streport__chip">
                  {file.name}
                  <button
                    type="button"
                    className="streport__chipx"
                    aria-label={`Remove ${file.name}`}
                    onClick={() => remove(file.id)}
                  >
                    &#x2715;
                  </button>
                </span>
              ))}
            </div>
          )}
          <button
            type="button"
            className="streport__addfile"
            onClick={() => inputRef.current?.click()}
            disabled={evidence.length >= REPORT_LIMITS.maxFiles}
          >
            Add File
          </button>
        </div>
        {showError && (
          <div className="streport__fielderror">
            Please upload the evidence of your issue
          </div>
        )}
      </div>

      <WizardControls
        onBack={onBack}
        primaryLabel="Continue"
        onPrimary={() => {
          setTouched(true);
          onContinue();
        }}
      />
    </>
  );
}

function ReviewStep({
  draft,
  reasonOptions,
  onConfirm,
  onBack,
  onSubmit,
}: {
  draft: ReportDraft;
  reasonOptions: ReasonOption[];
  onConfirm: (confirmAccuracy: boolean) => void;
  onBack: () => void;
  onSubmit: () => void;
}) {
  const reasonLabel =
    reasonOptions.find((o) => o.value === draft.reason)?.label ?? draft.reason;

  return (
    <>
      <div className="streport__field">
        <div className="streport__label">Review your report</div>
        <div className="streport__helper">
          Confirm the details below, then submit. Your wallet signs the
          submission, and moderators can see the report and its evidence.
        </div>
      </div>

      <ReviewRow label="Reported wallet" value={draft.reportedAddress || "\u{2014}"} />
      <ReviewRow label="Reason" value={reasonLabel || "\u{2014}"} />
      <ReviewRow label="Description" value={draft.description || "\u{2014}"} />
      <ReviewRow
        label="Evidence"
        value={
          draft.evidence.length
            ? draft.evidence.map((e) => e.name).join(", ")
            : "\u{2014}"
        }
      />

      <div className="streport__field">
        <label className="streport__confirm">
          <input
            type="checkbox"
            className="streport__checkboxinput"
            checked={draft.confirmAccuracy}
            onChange={(e) => onConfirm(e.target.checked)}
          />
          <span
            className={
              "streport__checkbox" + (draft.confirmAccuracy ? " is-checked" : "")
            }
          >
            {draft.confirmAccuracy && (
              <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true">
                <path
                  d="M3 8.5l3 3 7-7"
                  fill="none"
                  stroke="#fff"
                  strokeWidth="2.2"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                />
              </svg>
            )}
          </span>
          <span className="streport__confirmlabel">
            I confirm this report is accurate.
          </span>
        </label>
      </div>

      <WizardControls
        onBack={onBack}
        primaryLabel="Submit Report"
        primaryDisabled={!draft.confirmAccuracy}
        onPrimary={onSubmit}
      />
    </>
  );
}

function ReviewRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="streport__field">
      <div className="streport__label">{label}</div>
      <div className="streport__helper" style={{ wordBreak: "break-word" }}>
        {value}
      </div>
    </div>
  );
}

function SubmittingStep() {
  return (
    <div className="streport__field" role="status" aria-live="polite">
      <div className="streport__label">Submitting your report&#x2026;</div>
      <div className="streport__helper">
        Uploading evidence and finalizing the report. This usually takes a
        moment.
      </div>
      <button type="button" className="streport__submit" disabled>
        Submitting...
      </button>
    </div>
  );
}

function ErrorStep({
  message,
  onRetry,
}: {
  message?: string;
  onRetry: () => void;
}) {
  return (
    <>
      <div className="streport__submiterror">
        We couldn't submit your report. Please try again.
        {message ? ` (${message})` : ""}
      </div>
      <WizardControls primaryLabel="Retry" onPrimary={onRetry} />
    </>
  );
}
