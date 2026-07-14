import { useState, type CSSProperties, type ReactElement } from "react";
import { GovernanceChromeMaybe, type GovernanceNavId } from "../frames/GovernanceChrome";
import "./gvprojectupdatedetail.css";
import { ChevronLeft } from "../../atoms/icons";

type VarStyle = CSSProperties & { [k: `--${string}`]: string | number };
const cssVars = (s: VarStyle): CSSProperties => s;

type FinancialRecord = { description: string; receiver: string; token: string; amount: number; link: string };
type FinancialGroup = { category: string; records: FinancialRecord[] };
type Funds = {
  released: string;
  releasedTxCount: number;
  releasedTime: string;
  disclosed: string;
  undisclosed: string;
};
export type GvProjectUpdate = {
  id: string;
  index: number;
  status: string;
  health: string;
  author: string;
  completion_date: string;
  updated_at: string;
  due_date: string;
  due_amount: string;
  introduction: string;
  highlights: string[];
  blockers: string;
  next_steps: string;
  additional_notes: string;
  financial_records: FinancialGroup[];
  funds: Funds;
  discourse_topic_id: number;
};
type Update = GvProjectUpdate;

export type GvUpdateProject = { id: string; title: string; authorHue: number };
type Project = GvUpdateProject;

export type GvUpdateComment = {
  id: number;
  name: string;
  hue: number;
  validated: boolean;
  time: string;
  text: string;
};

const HEALTH_COPY: Record<string, { title: string; description: string }> = {
  onTrack: {
    title: "Project Health: On Track",
    description: "This team has indicated that the project is overall going according to plan.",
  },
  atRisk: {
    title: "Project Health: At Risk",
    description: "This team has indicated that the project plan is at risk or delayed.",
  },
  offTrack: {
    title: "Project Health: Off Track",
    description: "This team has indicated that the project is not going according to plan.",
  },
};

const CheckCircle = () => (
  <svg viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <circle cx="12" cy="12" r="9" />
    <path d="m8.5 12 2.5 2.5 4.5-5" />
  </svg>
);
const WarningIcon = () => (
  <svg viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d="M12 3 2 20h20L12 3Z" />
    <path d="M12 9v5M12 17h.01" />
  </svg>
);
const CancelIcon = () => (
  <svg viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <circle cx="12" cy="12" r="9" />
    <path d="m9 9 6 6M15 9l-6 6" />
  </svg>
);
const IncomeArrow = () => (
  <svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d="M7 7h10v10" />
    <path d="M7 17 17 7" />
  </svg>
);
const OutcomeArrow = () => (
  <svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d="M17 7H7v10" />
    <path d="M17 7 7 17" />
  </svg>
);
const Chevron = ({ open }: { open: boolean }) => (
  <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" style={{ transform: open ? "rotate(180deg)" : "none", transition: "transform .15s" }}>
    <path d="M4 6l4 4 4-4" />
  </svg>
);
const LinkOut = () => (
  <svg viewBox="0 0 16 16" width="12" height="12" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d="M6 3H3v10h10v-3" />
    <path d="M9 3h4v4M13 3 7 9" />
  </svg>
);
const ValidatedBadge = () => (
  <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true" className="gud__validated">
    <path d="M8 1.2 9.8 3l2.5-.3.6 2.4 2 1.5-1.3 2.1 1.3 2.1-2 1.5-.6 2.4-2.5-.3L8 14.8 6.2 13l-2.5.3-.6-2.4-2-1.5L2.4 7.3 1.1 5.2l2-1.5.6-2.4L6.2 3 8 1.2Z" fill="var(--brand)" />
    <path d="m5.5 8 1.7 1.7L10.6 6" fill="none" stroke="#fff" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" />
  </svg>
);

const HEALTH_ICON: Record<string, () => ReactElement> = { onTrack: CheckCircle, atRisk: WarningIcon, offTrack: CancelIcon };

function HealthBanner({ health }: { health: string }) {
  const copy = HEALTH_COPY[health];
  const Icon = HEALTH_ICON[health];
  if (!copy || !Icon) return null;
  return (
    <div className={`gud__health gud__health--${health}`}>
      <span className="gud__healthicon" aria-hidden="true">
        <Icon />
      </span>
      <div>
        <span className={`gud__healthtitle gud__healthtitle--${health}`}>{copy.title}</span>
        <p className="gud__healthdesc">{copy.description}</p>
      </div>
    </div>
  );
}

function FinancialCard({
  type,
  title,
  value,
  subtitle,
}: {
  type: string;
  title: string;
  value: string;
  subtitle?: string;
}) {
  return (
    <div className="gud__fincard">
      <span className="gud__fincardlabel">{title}</span>
      <div className="gud__fincardvalue">
        <span className={`gud__finarrow gud__finarrow--${type}`} aria-hidden="true">
          {type === "income" ? <IncomeArrow /> : <OutcomeArrow />}
        </span>
        <span className="gud__fincardamount">{value}</span>
      </div>
      {subtitle && <span className="gud__fincardsub">{subtitle}</span>}
    </div>
  );
}

function BreakdownRow({ group }: { group: FinancialGroup }) {
  const [open, setOpen] = useState(false);
  const total = group.records.reduce((s, r) => s + r.amount, 0);
  const fmt = (n: number) => "$" + n.toLocaleString("en-US", { minimumFractionDigits: 2, maximumFractionDigits: 2 });
  return (
    <div className={"gud__breakdown" + (open ? " is-open" : "")}>
      <button type="button" className="gud__breakdownhead" aria-expanded={open} onClick={() => setOpen((o) => !o)}>
        <span className="gud__breakdowntitle">{group.category}</span>
        <span className="gud__breakdownsub">
          {group.records.length} {group.records.length === 1 ? "item" : "items"}
        </span>
        <span className="gud__breakdownvalue">{fmt(total)}</span>
        <Chevron open={open} />
      </button>
      {open && (
        <div className="gud__breakdownbody">
          {group.records.map((r, i) => (
            <div className="gud__sumitem" key={i}>
              <div className="gud__sumdesc">
                <span className="gud__sumdesctext">{r.description}</span>
                <span className="gud__sumreceiver">{r.receiver}</span>
              </div>
              <div className="gud__sumdetails">
                <span className="gud__sumtoken">{r.token}</span>
                <span className="gud__sumamount">{fmt(r.amount)}</span>
                {r.link ? (
                  <a className="gud__sumlink" href={r.link} target="_blank" rel="noopener noreferrer" aria-label="View transaction">
                    <LinkOut />
                  </a>
                ) : (
                  <span className="gud__sumlink gud__sumlink--hidden" aria-hidden="true">
                    <LinkOut />
                  </span>
                )}
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function MarkdownList({ items }: { items: string[] }) {
  return (
    <ul className="gud__list">
      {items.map((it, i) => (
        <li className="gud__listitem" key={i}>
          {it}
        </li>
      ))}
    </ul>
  );
}

export default function GvProjectUpdateDetail({
  chrome = true,
  update,
  project,
  comments = [],
  state = "ready",
}: {
  chrome?: boolean;
  update?: Update;
  project?: Project;
  comments?: GvUpdateComment[];
  state?: string;
}) {
  const [tab, setTab] = useState<GovernanceNavId>("projects");

  if (state === "loading") {
    return (
      <GovernanceChromeMaybe chrome={chrome} active={tab} onTab={setTab}>
        <div className="gud gud--center">
          <div className="gud__loading" role="status" aria-live="polite">
            <span className="gud__spinner" aria-hidden="true" />
          </div>
        </div>
      </GovernanceChromeMaybe>
    );
  }

  if (state === "error" || !update || !project) {
    return (
      <GovernanceChromeMaybe chrome={chrome} active={tab} onTab={setTab}>
        <div className="gud">
          <div className="gud__notfound" role="alert">
            <div className="gud__notfoundtitle">Not found</div>
            <p className="gud__notfoundsub">{"You just hit a route that doesn't exist\u{2026}"}</p>
          </div>
        </div>
      </GovernanceChromeMaybe>
    );
  }

  const hasFinancials = update.financial_records && update.financial_records.length > 0;
  const edited = update.updated_at && update.updated_at !== update.completion_date;

  return (
    <GovernanceChromeMaybe chrome={chrome} active={tab} onTab={setTab}>
      <div className="gud">
        <div className="gud__layout">
          <div className="gud__back">
            <button type="button" className="gud__backbtn" aria-label="Back" onClick={() => setTab("projects")}>
              <ChevronLeft size={14} />
            </button>
          </div>

          <div className="gud__col">
            <section className="gud__section gud__header">
              <span className="gud__projecttitle">
                Project update for{" "}
                <a className="gud__projectlink" href={`/governance/projects/${project.id}`}>
                  {project.title}
                </a>
              </span>
              <h1 className="gud__title">Update #{update.index}</h1>
            </section>

            <section className="gud__section gud__content">
              <HealthBanner health={update.health} />

              <h2 className="gud__heading">Introduction</h2>
              <p className="gud__text">{update.introduction}</p>

              <h2 className="gud__heading">Highlights</h2>
              <MarkdownList items={update.highlights} />

              <h2 className="gud__heading">Blockers</h2>
              <p className="gud__text">{update.blockers}</p>

              <h2 className="gud__heading">Next steps</h2>
              <p className="gud__text">{update.next_steps}</p>

              {update.additional_notes && (
                <>
                  <h2 className="gud__heading">Additional notes and links</h2>
                  <p className="gud__text">{update.additional_notes}</p>
                </>
              )}

              {hasFinancials && (
                <>
                  <h2 className="gud__heading">Financial details</h2>
                  <div className="gud__financial">
                    <div className="gud__fincards">
                      <FinancialCard
                        type="income"
                        title="Funds released since last update"
                        value={update.funds.released}
                        subtitle={`${update.funds.releasedTxCount} tx. last one made ${update.funds.releasedTime}`}
                      />
                      <FinancialCard
                        type="outcome"
                        title="Funds disclosed this update"
                        value={update.funds.disclosed}
                        subtitle={`${update.funds.undisclosed} left undisclosed`}
                      />
                    </div>
                    <div className="gud__summary">
                      {update.financial_records.map((g) => (
                        <BreakdownRow key={g.category} group={g} />
                      ))}
                    </div>
                  </div>
                </>
              )}

              <hr className="gud__divider" />
              <div className="gud__dates">
                <div className="gud__dateline">
                  <span className="gud__datetext">Posted {update.completion_date} by</span>
                  <span className="gud__author" role="button" tabIndex={0} onClick={(e) => e.preventDefault()}>
                    <span className="gud__authoravatar u-avatar" style={cssVars({ "--sz": "20px", "--hue": project.authorHue })} aria-hidden="true" />
                    {update.author}
                  </span>
                </div>
                {edited && (
                  <div className="gud__dateline">
                    <span className="gud__datetext">
                      Update last edited <strong>{update.updated_at}</strong>
                    </span>
                  </div>
                )}
                {update.status === "late" && (
                  <div className="gud__dateline">
                    <span className="gud__datetext">
                      Update shared <strong>{update.due_amount} late</strong>, after it was due.
                    </span>
                  </div>
                )}
              </div>
            </section>

            <section className="gud__section gud__commentsec">
              <div className="gud__commentsbar">
                <h2 className="gud__commentstitle">
                  {comments.length} {comments.length === 1 ? "Comment" : "Comments"}
                </h2>
                <button type="button" className="gud__joinbtn">
                  Join the discussion
                </button>
              </div>
              <div className="gud__comments">
                {comments.map((c) => (
                  <div className="gud__comment" key={c.id}>
                    <span className="gud__commentavatar u-avatar" style={cssVars({ "--sz": "40px", "--hue": c.hue })} aria-hidden="true" />
                    <div className="gud__commentbody">
                      <div className="gud__commenthead">
                        <span className="gud__commentname" role="button" tabIndex={0} onClick={(e) => e.preventDefault()}>
                          {c.name}
                          {c.validated && <ValidatedBadge />}
                        </span>
                        <span className="gud__commenttime">{c.time}</span>
                      </div>
                      <p className="gud__commenttext">{c.text}</p>
                    </div>
                  </div>
                ))}
              </div>
              <button type="button" className="gud__readmore">
                Comment on this Update
              </button>
            </section>
          </div>
        </div>
      </div>
    </GovernanceChromeMaybe>
  );
}
