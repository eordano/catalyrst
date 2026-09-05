import { useState } from "react";
import type { FormEvent, ReactNode } from "react";
import { GovernanceChromeMaybe } from "../frames/GovernanceChrome";
import { charCounter } from "../components/SubmitFormShared";
import "./gvsubmitpitch.css";

type PitchForm = {
  initiative_name: string;
  problem_statement: string;
  proposed_solution: string;
  target_audience: string;
  relevance: string;
};

type PitchFieldKey = "problem_statement" | "proposed_solution" | "target_audience" | "relevance";

const LIMITS: Record<keyof PitchForm, { min: number; max: number }> = {
  initiative_name: { min: 1, max: 80 },
  problem_statement: { min: 20, max: 3500 },
  proposed_solution: { min: 20, max: 3500 },
  target_audience: { min: 20, max: 3500 },
  relevance: { min: 20, max: 3500 },
};

const COPY = {
  title: "Pitch proposal",
  description:
    "Pitch proposals are the first step towards validating whether an idea is worth pursuing as a Project Tender and securing funds and bids from external teams to execute it.",
  description2line:
    "This action requires at least 100 VP. Buy MANA to get VP, or run for delegate.",
  initiative_name_label: "Initiative name",
  initiative_name_postlabel: "This is what will be displayed on the Proposals Feed",
  problem_statement_label: "Problem statement",
  problem_statement_detail:
    "Please explain the problem this initiative would solve in using simple words",
  proposed_solution_label: "Proposed solution",
  proposed_solution_detail:
    "Please explain how you would think this problem should be approached using simple words",
  target_audience_label: "Target audience",
  target_audience_detail:
    "Please describe the intended user of the proposed solution: their needs, concerns, motivations, etc.",
  relevance_label: "Why is this relevant now?",
  relevance_detail:
    "Please explain why the Decentraland DAO should be spending money on this project. Specify market conditions, competitors, new technical developments, and any other relevant factors you consider",
  co_author_label: "Co-authors",
  co_author_optional: "(optional)",
  co_author_description:
    "Add the addresses of any co-authors who contributed to this proposal.",
  co_author_placeholder: "Add a co-author address",
  button_submit: "Submit proposal",
  vp_not_met: "This action requires at least 100 VP.",
  error_label: "There was an error.",
  markdown_tooltip: " (markdown)",
};

const MD_FIELDS: { key: PitchFieldKey; label: string; detail: string; placeholder: string }[] = [
  {
    key: "problem_statement",
    label: COPY.problem_statement_label,
    detail: COPY.problem_statement_detail,
    placeholder: "Describe the problem this initiative would solve\u{2026}",
  },
  {
    key: "proposed_solution",
    label: COPY.proposed_solution_label,
    detail: COPY.proposed_solution_detail,
    placeholder: "Describe how this problem should be approached\u{2026}",
  },
  {
    key: "target_audience",
    label: COPY.target_audience_label,
    detail: COPY.target_audience_detail,
    placeholder: "Describe the intended user of the proposed solution\u{2026}",
  },
  {
    key: "relevance",
    label: COPY.relevance_label,
    detail: COPY.relevance_detail,
    placeholder: "Explain why the DAO should spend money on this now\u{2026}",
  },
];

const MarkdownNotice = () => (
  <sup className="gsp__markdown" title="You can format using markdown!">
    {COPY.markdown_tooltip}
  </sup>
);

const FieldLabel = ({ children, htmlFor }: { children: ReactNode; htmlFor?: string }) => (
  <label className="gsp__label" htmlFor={htmlFor}>
    {children}
    <MarkdownNotice />
  </label>
);

const SubLabel = ({ children }: { children: ReactNode }) => <p className="gsp__sublabel">{children}</p>;

const MessageRow = ({ value, limit, error }: { value: string; limit: number; error?: boolean }) => (
  <div className={"gsp__msgrow" + (error ? " is-error" : "")}>
    <span className="gsp__msg">{charCounter((value || "").length, limit)}</span>
  </div>
);

const MD_COMMANDS = ["B", "I", "S", "H", "\u{201C}", "\u{21A9}", "\u{2022}", "1.", "\u{1F517}", "\u{1F5BC}", "</>"];

function MarkdownEditor({
  name,
  id,
  value,
  placeholder,
  disabled,
  error,
  onChange,
}: {
  name: string;
  id?: string;
  value: string;
  placeholder?: string;
  disabled?: boolean;
  error?: boolean;
  onChange?: (value: string) => void;
}) {
  return (
    <div className={"gsp__md" + (error ? " is-error" : "") + (disabled ? " is-disabled" : "")}>
      <div className="gsp__mdtoolbar">
        <div className="gsp__mdcmds">
          {MD_COMMANDS.map((c, i) => (
            <button
              key={i}
              type="button"
              className="gsp__mdcmd"
              tabIndex={-1}
              disabled={disabled}
              aria-hidden="true"
            >
              {c}
            </button>
          ))}
        </div>
        <div className="gsp__mdextra">
          <button type="button" className="gsp__mdtoggle" tabIndex={-1} disabled={disabled}>
            Preview
          </button>
          <button type="button" className="gsp__mdtoggle" tabIndex={-1} disabled={disabled}>
            &#x2922;
          </button>
        </div>
      </div>
      <textarea
        className="gsp__mdtext"
        name={name}
        id={id}
        value={value}
        placeholder={placeholder}
        disabled={disabled}
        rows={6}
        onChange={(e) => onChange?.(e.target.value)}
      />
    </div>
  );
}

function CoAuthors({ disabled }: { disabled?: boolean }) {
  return (
    <div className="gsp__coauthors">
      <div className="gsp__labelrow">
        <span className="gsp__label gsp__label--inline">{COPY.co_author_label}</span>
        <sup className="gsp__optional">{COPY.co_author_optional}</sup>
      </div>
      <SubLabel>{COPY.co_author_description}</SubLabel>
      <div className={"gsp__addrselect" + (disabled ? " is-disabled" : "")}>
        <svg viewBox="0 0 16 16" width="15" height="15" aria-hidden="true" className="gsp__addricon">
          <circle cx="6.5" cy="6.5" r="4.5" fill="none" stroke="currentColor" strokeWidth="1.5" />
          <path d="M10 10l4 4" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
        </svg>
        <input
          className="gsp__addrinput"
          type="text"
          placeholder={COPY.co_author_placeholder}
          aria-label={COPY.co_author_label}
          disabled={disabled}
        />
      </div>
    </div>
  );
}

function ErrorBanner({ label, message }: { label: string; message: string }) {
  const [open, setOpen] = useState(false);
  return (
    <div className="gsp__error">
      <div className="gsp__errhead">
        <span className="gsp__errnotice">
          <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
            <circle cx="12" cy="12" r="10" fill="none" stroke="currentColor" strokeWidth="2" />
            <path d="M12 7v6" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
            <circle cx="12" cy="16.5" r="1.1" fill="currentColor" />
          </svg>
          <span className="gsp__errlabel">{label}</span>
        </span>
        <button type="button" className="gsp__errshow" onClick={() => setOpen((v) => !v)}>
          {open ? "Hide" : "Show"}
        </button>
      </div>
      {open && (
        <div className="gsp__errbody">
          <pre className="gsp__errpre">{message}</pre>
        </div>
      )}
    </div>
  );
}

function LoginGate({ chrome, onTab }: { chrome?: boolean; onTab?: (id: string) => void }) {
  return (
    <GovernanceChromeMaybe chrome={chrome} active="proposals" onTab={onTab} account="Sign in">
      <div className="gsp gsp--gate">
        <div className="gsp__signin">
          <h1 className="gsp__signintitle">Sign In</h1>
          <p className="gsp__signinsub">
            You need to sign in to submit a pitch.
          </p>
          <button type="button" className="gsp__signinbtn">Connect</button>
        </div>
      </div>
    </GovernanceChromeMaybe>
  );
}

function LoadingGate({ chrome, onTab }: { chrome?: boolean; onTab?: (id: string) => void }) {
  return (
    <GovernanceChromeMaybe chrome={chrome} active="proposals" onTab={onTab}>
      <div className="gsp gsp--gate">
        <div className="gsp__loading" role="status" aria-label="Loading">
          <span className="gsp__spinner" />
        </div>
      </div>
    </GovernanceChromeMaybe>
  );
}

type GvSubmitPitchProps = {
  chrome?: boolean;
  account?: string;
  loading?: boolean;
  vpNotMet?: boolean;
  error?: string;
};

export default function GvSubmitPitch({
  chrome = true,
  account = "",
  loading = false,
  vpNotMet = false,
  error = "",
}: GvSubmitPitchProps) {
  const [form, setForm] = useState<PitchForm>({
    initiative_name: "",
    problem_statement: "",
    proposed_solution: "",
    target_audience: "",
    relevance: "",
  });
  const set = (k: keyof PitchForm) => (v: string) => setForm((f) => ({ ...f, [k]: v }));
  const disabled = vpNotMet;

  const handleSubmit = (e: FormEvent<HTMLFormElement>) => e.preventDefault();

  if (loading) return <LoadingGate chrome={chrome} />;
  if (!account) return <LoginGate chrome={chrome} />;

  return (
    <GovernanceChromeMaybe chrome={chrome} active="proposals">
      <div className="gsp">
        <form className="gsp__form" onSubmit={handleSubmit}>
          <section className="gsp__section">
            <h1 className="gsp__title">{COPY.title}</h1>
          </section>

          <section className="gsp__section">
            <p className="gsp__lead">{COPY.description}</p>
            <p className="gsp__lead">{COPY.description2line}</p>
          </section>

          <section className="gsp__section">
            <FieldLabel htmlFor="gsp-initiative_name">{COPY.initiative_name_label}</FieldLabel>
            <input
              className={"gsp__input" + (disabled ? " is-disabled" : "")}
              type="text"
              name="initiative_name"
              id="gsp-initiative_name"
              value={form.initiative_name}
              maxLength={LIMITS.initiative_name.max}
              disabled={disabled}
              onChange={(e) => set("initiative_name")(e.target.value)}
            />
            <MessageRow value={form.initiative_name} limit={LIMITS.initiative_name.max} />
            <p className="gsp__postlabel">{COPY.initiative_name_postlabel}</p>
          </section>

          {MD_FIELDS.map((f) => (
            <section className="gsp__section" key={f.key}>
              <FieldLabel htmlFor={"gsp-" + f.key}>{f.label}</FieldLabel>
              <SubLabel>{f.detail}</SubLabel>
              <MarkdownEditor
                name={f.key}
                id={"gsp-" + f.key}
                value={form[f.key]}
                placeholder={f.placeholder}
                disabled={disabled}
                onChange={set(f.key)}
              />
              <MessageRow value={form[f.key]} limit={LIMITS[f.key].max} />
            </section>
          ))}

          <section className="gsp__section">
            <CoAuthors disabled={disabled} />
          </section>

          <section className="gsp__section">
            <button type="submit" className="gsp__submit" disabled={disabled}>
              {COPY.button_submit}
            </button>
          </section>

          {vpNotMet && (
            <section className="gsp__section">
              <p className="gsp__vpnotice">{COPY.vp_not_met}</p>
            </section>
          )}

          {error && (
            <section className="gsp__section">
              <ErrorBanner label={COPY.error_label} message={error} />
            </section>
          )}
        </form>
      </div>
    </GovernanceChromeMaybe>
  );
}
