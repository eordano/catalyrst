import { useState, type CSSProperties, type ReactNode } from "react";
import { GovernanceChromeMaybe, type GovernanceNavId } from "../frames/GovernanceChrome";
import Button from "../../atoms/Button";
import EmptyState from "../../components/EmptyState";
import "./gvproposaldetail.css";

type VarStyle = CSSProperties & { [k: `--${string}`]: string | number };
const cssVars = (s: VarStyle): CSSProperties => s;

type Budget = { size: string; beneficiary: string; tier: string };
export type GvProposal = {
  id: string;
  type: string;
  toneClass: string;
  catLabel: string;
  catTone: string;
  status: string;
  statusLabel: string;
  statusTone: string;
  title: string;
  author: string;
  authorHue: number;
  published: string;
  start: string;
  finish: string;
  snapshot: string;
  threshold: string;
  thresholdReached: boolean;
  yourVp: string;
  budget: Budget;
  description?: string;
};
type Proposal = GvProposal;

export type GvVoteChoice = {
  id: string;
  label: string;
  pct: number;
  vp: string;
  tone: string;
  voted: boolean;
};

export type GvSurveyRow = {
  id: string;
  label: string;
  emoji: string;
  count: number;
  pct: number;
  dir: string;
};

export type GvVoteRationale = {
  id: number;
  name: string;
  hue: number;
  choice: string;
  tone: string;
  vp: string;
  text: string;
};

export type GvProposalComment = {
  id: number;
  name: string;
  hue: number;
  time: string;
  text: string;
};

export type GvVpSeries = { yes: number[]; no: number[]; ticks: string[] };

const OpenIcon = () => (
  <svg viewBox="0 0 16 16" width="12" height="12" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d="M6 3H3v10h10v-3" />
    <path d="M9 3h4v4M13 3 7 9" />
  </svg>
);
const VpBolt = () => (
  <svg viewBox="0 0 24 24" width="12" height="12" aria-hidden="true">
    <path d="M13 2 4 14h6l-1 8 9-12h-6l1-8Z" fill="currentColor" />
  </svg>
);
const DiscourseIcon = () => (
  <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2Z" />
  </svg>
);

function Pill({ kind, tone, children }: { kind: string; tone?: string; children: ReactNode }) {
  const style: (CSSProperties & { "--tone-t": string; "--tone-bg": string }) | undefined =
    tone && tone !== "neutral"
      ? { "--tone-t": `var(--tone-${tone}-t)`, "--tone-bg": `var(--tone-${tone}-bg)` }
      : undefined;
  return (
    <span className={`gd__pill gd__pill--${kind}`} style={style}>
      {children}
    </span>
  );
}

function ProposalHero({ proposal }: { proposal: Proposal }) {
  const active = proposal.status === "active";
  return (
    <section className={"gd__hero" + (active ? "" : " gd__hero--finished")} aria-label="Proposal header">
      <div
        className={
          "gd__herobanner " +
          (active ? `gd__herobanner--${proposal.toneClass}` : "gd__herobanner--finished")
        }
        aria-hidden="true"
      />
      <div className="gd__herotext">
        <h1 className="gd__herotitle">{proposal.title}</h1>
        <div className="gd__herolabels">
          {active ? (
            <Pill kind="white">{proposal.statusLabel}</Pill>
          ) : (
            <Pill kind="status" tone={proposal.statusTone}>
              <span className="gd__pilldot" />
              {proposal.statusLabel}
            </Pill>
          )}
          {active ? (
            <Pill kind="ghost">{proposal.catLabel}</Pill>
          ) : (
            <Pill kind="cat" tone={proposal.catTone}>
              {proposal.catLabel}
            </Pill>
          )}
        </div>
      </div>
    </section>
  );
}

function DetailsCard({ proposal }: { proposal: Proposal }) {
  return (
    <div className="gd__detailcard">
      <h2 className="gd__seclabel">Proposal Details</h2>
      <div className="gd__row">
        <span className="gd__rowkey">Author</span>
        <span className="gd__author">
          <span className="gd__av u-avatar" style={cssVars({ "--sz": "20px", "--hue": proposal.authorHue })} aria-hidden="true" />
          {proposal.author}
        </span>
      </div>
      <div className="gd__row">
        <span className="gd__rowkey">Published</span>
        <span className="gd__rowval">{proposal.published}</span>
      </div>
      <div className="gd__row">
        <span className="gd__rowkey">Voting begins</span>
        <span className="gd__rowval">{proposal.start}</span>
      </div>
      <div className="gd__row">
        <span className="gd__rowkey">Voting ends</span>
        <span className="gd__rowval">{proposal.finish}</span>
      </div>
      <div className="gd__row">
        <span className="gd__rowkey">Snapshot</span>
        <a className="gd__snaplink" href="https://snapshot.org/#/dao-council.dcl.eth" target="_blank" rel="noopener noreferrer">
          {proposal.snapshot}
          <OpenIcon />
        </a>
      </div>
    </div>
  );
}

function VpChart({ threshold = "2,000,000", series }: { threshold?: string; series: GvVpSeries }) {
  const thr = Number(String(threshold).replace(/[^0-9]/g, "")) || 2000000;
  const w = 560, h = 232, padL = 50, padR = 12, padT = 14, padB = 24;
  const ymax = thr * 1.12;
  const n = series.yes.length;
  const X = (i: number) => padL + ((w - padL - padR) * i) / (n - 1);
  const Y = (v: number) => padT + (h - padT - padB) * (1 - v / ymax);
  const line = (a: number[]) => a.map((v, i) => (i ? "L" : "M") + X(i).toFixed(1) + "," + Y(v).toFixed(1)).join(" ");
  const area = (a: number[]) => line(a) + ` L${X(n - 1).toFixed(1)},${Y(0).toFixed(1)} L${X(0).toFixed(1)},${Y(0).toFixed(1)} Z`;
  const ty = Y(thr);
  return (
    <div className="gd__block">
      <h2 className="gd__blocktitle">Voting Power</h2>
      <svg viewBox={`0 0 ${w} ${h}`} className="gd__vpsvg" role="img" aria-label="Voting power over time">
        {[0, thr / 2, thr].map((v) => (
          <g key={v}>
            <line x1={padL} y1={Y(v)} x2={w - padR} y2={Y(v)} className="gd__vpgrid" />
            <text x={padL - 8} y={Y(v) + 4} textAnchor="end" className="gd__vptick">{v ? v / 1e6 + "M" : "0"}</text>
          </g>
        ))}
        <path d={area(series.no)} className="gd__vparea gd__vparea--no" />
        <path d={area(series.yes)} className="gd__vparea gd__vparea--yes" />
        <path d={line(series.no)} className="gd__vpline gd__vpline--no" />
        <path d={line(series.yes)} className="gd__vpline gd__vpline--yes" />
        <line x1={padL} y1={ty} x2={w - padR} y2={ty} className="gd__vpthr" />
        <text x={w - padR} y={ty - 6} textAnchor="end" className="gd__vpthrlabel">Required to pass</text>
        {series.ticks.length > 1 && (() => {
          const total = series.ticks.length;
          const count = Math.min(5, total);
          const idxs = Array.from(
            new Set(
              Array.from({ length: count }, (_, k) =>
                Math.round((k * (total - 1)) / (count - 1)),
              ),
            ),
          );
          return idxs.map((i) => (
            <text key={i} x={padL + ((w - padL - padR) * i) / (total - 1)} y={h - 6}
              textAnchor={i === 0 ? "start" : i === total - 1 ? "end" : "middle"} className="gd__vptick">{series.ticks[i]}</text>
          ));
        })()}
      </svg>
      <div className="gd__vplegend">
        <span className="gd__vpleg"><i className="gd__vpdot gd__vpdot--yes" />Yes</span>
        <span className="gd__vpleg"><i className="gd__vpdot gd__vpdot--no" />No</span>
      </div>
    </div>
  );
}

function VoteBox({
  proposal,
  choices,
  totalVotesLabel,
  active,
  voted,
  onVote,
  selected,
  onSelect,
}: {
  proposal: Proposal;
  choices: GvVoteChoice[];
  totalVotesLabel?: string;
  active: boolean;
  voted: boolean;
  onVote: () => void;
  selected: string | null;
  onSelect: (id: string) => void;
}) {
  return (
    <aside className="gd__sidebar" aria-label="Voting">
      <div className="gd__votebox">
        <div className="gd__threshold">
          <p className="gd__thsub">Acceptance Threshold</p>
          <div className="gd__thtitle">{proposal.threshold} VP</div>
          <div className="gd__thdots">
            <span className="gd__thdot is-active" />
            <span className="gd__thdot" />
          </div>
        </div>

        <div className="gd__vprow">
          <span className="gd__rowkey">Your Voting Power</span>
          <b>
            <VpBolt /> {proposal.yourVp} VP
          </b>
        </div>

        <div className="gd__choices" role="radiogroup" aria-label="Vote choices">
          {choices.map((c) => (
            <button
              key={c.id}
              type="button"
              role="radio"
              aria-checked={selected === c.id}
              className={
                "gd__choice" +
                (voted && c.voted ? " is-voted" : "") +
                (selected === c.id ? " is-selected" : "")
              }
              disabled={!active}
              onClick={() => onSelect(c.id)}
            >
              <span className={`gd__choicefill gd__choicefill--${c.tone}`} style={{ width: c.pct + "%" }} aria-hidden="true" />
              <span className="gd__choicetxt">{c.label}</span>
              <span className="gd__choicepct">{c.pct}%</span>
            </button>
          ))}
        </div>

        {active ? (
          <Button variant="primary" className="gd__votecta" onClick={onVote} disabled={!selected}>
            Cast Vote
          </Button>
        ) : (
          <div className="gd__voteclosed">Voting closed</div>
        )}

        <div className="gd__votingstate">
          <span className={active ? "is-open" : ""}>{active ? "Open for votes" : "Voting finished"}</span>
          {totalVotesLabel ? <span>{totalVotesLabel}</span> : null}
        </div>
      </div>
    </aside>
  );
}

export default function GvProposalDetail({
  chrome = true,
  proposal,
  choices = [],
  survey = [],
  rationales = [],
  comments = [],
  commentsTotal,
  vpSeries,
  totalVotesLabel,
  forumUrl,
  state = "ready",
}: {
  chrome?: boolean;
  proposal?: Proposal;
  choices?: GvVoteChoice[];
  survey?: GvSurveyRow[];
  rationales?: GvVoteRationale[];
  comments?: GvProposalComment[];
  commentsTotal?: number;
  vpSeries?: GvVpSeries;
  totalVotesLabel?: string;
  forumUrl?: string;
  state?: string;
}) {
  const [tab, setTab] = useState<GovernanceNavId>("proposals");
  const [selected, setSelected] = useState<string | null>(null);
  const [voted, setVoted] = useState(true);

  if (state === "loading") {
    return (
      <GovernanceChromeMaybe chrome={chrome} active={tab} onTab={setTab}>
        <div className="gd">
          <div className="gd__loading" role="status" aria-live="polite">
            <span className="gd__spinner" aria-hidden="true" />
            Loading proposal&#x2026;
          </div>
        </div>
      </GovernanceChromeMaybe>
    );
  }

  if (state === "error" || !proposal) {
    return (
      <GovernanceChromeMaybe chrome={chrome} active={tab} onTab={setTab}>
        <div className="gd">
          <EmptyState
            className="gd__notfound"
            role="alert"
            tone="error"
            title="Proposal not found"
            subtitle="We couldn't find this proposal. It may have been deleted, or the link is incorrect."
            actions={[{ label: "Back to proposals", onClick: () => setTab("proposals"), variant: "ghost" }]}
          />
        </div>
      </GovernanceChromeMaybe>
    );
  }

  const active = proposal.status === "active";

  return (
    <GovernanceChromeMaybe chrome={chrome} active={tab} onTab={setTab}>
      <div className="gd">
        <nav className="gd__crumbs" aria-label="Breadcrumb">
          <button type="button" className="gd__crumb" onClick={() => setTab("proposals")}>
            Proposals
          </button>
          <span className="gd__crumbsep">/</span>
          <span className="gd__crumbcur">{proposal.catLabel}</span>
        </nav>

        <ProposalHero proposal={proposal} />

        <div className="gd__cols">
          <div className="gd__left">
            <DetailsCard proposal={proposal} />
          </div>

          <div className="gd__center">
            {proposal.type === "grant" && (
              <div className="gd__block">
                <div className="gd__budget">
                  <div className="gd__budgetcell">
                    <p className="gd__budgetlabel">Grant size</p>
                    <div className="gd__budgetval">{proposal.budget.size}</div>
                  </div>
                  <div className="gd__budgetcell">
                    <p className="gd__budgetlabel">Tier</p>
                    <div className="gd__budgetval">{proposal.budget.tier}</div>
                  </div>
                  <div className="gd__budgetcell">
                    <p className="gd__budgetlabel">Beneficiary</p>
                    <div className="gd__budgetval">{proposal.budget.beneficiary}</div>
                  </div>
                </div>
              </div>
            )}

            {proposal.description ? (
              <div className="gd__block gd__md">
                <h2>Description</h2>
                <p style={{ whiteSpace: "pre-wrap" }}>{proposal.description}</p>
              </div>
            ) : null}

            {vpSeries && vpSeries.yes.length > 1 ? (
              <VpChart threshold={proposal.threshold} series={vpSeries} />
            ) : null}

            {survey.length ? (
            <div className="gd__block">
              <h2 className="gd__blocktitle">Sentiment Survey</h2>
              <div className="gd__survey">
                {survey.map((s) => (
                  <div className="gd__surveyrow" key={s.id}>
                    <div className="gd__surveyhead">
                      <span className="gd__surveyq">
                        <span aria-hidden="true">{s.emoji}</span>
                        {s.label}
                      </span>
                      <span className="gd__surveycount">
                        {s.count} &#xB7; {s.pct}%
                      </span>
                    </div>
                    <div className="gd__surveytrack">
                      <span className={`gd__surveyfill gd__surveyfill--${s.dir}`} style={{ width: s.pct + "%" }} />
                    </div>
                  </div>
                ))}
              </div>
            </div>
            ) : null}

            {rationales.length ? (
            <div className="gd__block">
              <h2 className="gd__blocktitle">Voting Rationale</h2>
              <div className="gd__rationale">
                {rationales.map((r) => (
                  <div className="gd__rat" key={r.id}>
                    <span className="gd__av u-avatar" style={cssVars({ "--sz": "32px", "--hue": r.hue })} aria-hidden="true" />
                    <div className="gd__ratbody">
                      <div className="gd__rathead">
                        <span className="gd__ratname">{r.name}</span>
                        <span className={`gd__ratchoice gd__ratchoice--${r.tone}`}>{r.choice}</span>
                        <span className="gd__ratvp">{r.vp}</span>
                      </div>
                      <p className="gd__rattext">{r.text}</p>
                    </div>
                  </div>
                ))}
              </div>
            </div>
            ) : null}

            <div className="gd__block">
              <div className="gd__commentsbar">
                <h2 className="gd__blocktitle" style={{ margin: 0 }}>
                  {commentsTotal ?? comments.length} Comments
                </h2>
                {forumUrl ? (
                  <a className="gd__discoursebtn" href={forumUrl} target="_blank" rel="noopener noreferrer">
                    <DiscourseIcon />
                    Comment on this Proposal
                  </a>
                ) : null}
              </div>
              {comments.map((c) => (
                <div className="gd__comment" key={c.id}>
                  <span className="gd__av u-avatar" style={cssVars({ "--sz": "36px", "--hue": c.hue })} aria-hidden="true" />
                  <div className="gd__cbody">
                    <div className="gd__chead">
                      <span className="gd__cname">{c.name}</span>
                      <span className="gd__ctime">{c.time}</span>
                    </div>
                    <p className="gd__ctext">{c.text}</p>
                  </div>
                </div>
              ))}
            </div>
          </div>

          <div className="gd__right">
            <VoteBox
              proposal={proposal}
              choices={choices}
              totalVotesLabel={totalVotesLabel}
              active={active}
              voted={voted}
              selected={selected}
              onSelect={setSelected}
              onVote={() => setVoted(true)}
            />
          </div>
        </div>
      </div>
    </GovernanceChromeMaybe>
  );
}
