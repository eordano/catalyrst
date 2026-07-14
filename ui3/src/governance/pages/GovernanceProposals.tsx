import type { CSSProperties, ReactNode } from "react";
import { useMemo, useState } from "react";
import GovernanceChrome, { type GovernanceNavId } from "../frames/GovernanceChrome";
import SearchField from "../../atoms/SearchField";
import EmptyState from "../../components/EmptyState";
import "./governanceproposals.css";

const CAT_META: Record<string, { label: string; tone: string }> = {
  poi: { label: "Point of Interest", tone: "green" },
  catalyst: { label: "Catalyst Node", tone: "blue" },
  ban_name: { label: "Name Ban", tone: "fuchsia" },
  grant: { label: "Grant Request", tone: "purple" },
  linked_wearables: { label: "Linked Wearables Registry", tone: "yellow" },
  hiring: { label: "Hiring", tone: "green" },
  council_decision_veto: { label: "Council Decision Veto", tone: "red" },
  poll: { label: "Poll", tone: "orange" },
  draft: { label: "Draft", tone: "orange" },
  governance: { label: "Governance", tone: "orange" },
  pitch: { label: "Pitch", tone: "red" },
  tender: { label: "Tender", tone: "red" },
  bid: { label: "Bid", tone: "red" },
};

type CategoryFilter = { id: string; label: string; count: number; tone: string; members?: string[] };

const CATEGORIES: CategoryFilter[] = [
  { id: "all", label: "All proposals", count: 0, tone: "neutral" },
  { id: "poi", label: "Point of Interest", count: 0, tone: "green" },
  { id: "catalyst", label: "Catalyst Node", count: 0, tone: "blue" },
  { id: "ban_name", label: "Name Ban", count: 0, tone: "fuchsia" },
  { id: "linked_wearables", label: "Linked Wearables Registry", count: 0, tone: "yellow" },
  { id: "hiring", label: "Hiring", count: 0, tone: "green" },
  { id: "council_decision_veto", label: "Council Decision Veto", count: 0, tone: "red" },
  { id: "governance", label: "Governance Process", count: 0, tone: "orange", members: ["poll", "draft", "governance"] },
  { id: "grant", label: "Grant Request", count: 0, tone: "purple" },
  { id: "bidding", label: "Bidding and Tendering", count: 0, tone: "red", members: ["pitch", "tender", "bid"] },
];

const STATUSES: { id: string; label: string; tone: string }[] = [
  { id: "all", label: "All outcomes", tone: "neutral" },
  { id: "active", label: "Active", tone: "neutral" },
  { id: "passed", label: "Passed", tone: "green" },
  { id: "enacted", label: "Enacted", tone: "green" },
  { id: "rejected", label: "Rejected", tone: "red" },
  { id: "out_of_budget", label: "Out of Budget", tone: "yellow" },
  { id: "finished", label: "Finished", tone: "neutral" },
];

const TIMEFRAMES: { id: string; label: string }[] = [
  { id: "all", label: "All time" },
  { id: "week", label: "Last week" },
  { id: "month", label: "Last month" },
  { id: "3months", label: "Last 90 days" },
];

const SORTS: { id: string; label: string }[] = [
  { id: "DESC", label: "Latest" },
  { id: "ASC", label: "Oldest" },
];

export type GpProposal = {
  id: string | number;
  title: string;
  category: string;
  status: string;
  author: string;
  hue: number;
  forPct?: number;
  againstPct?: number;
  passing?: boolean;
  votes?: number;
  comments?: number;
  time: string;
  urgent?: boolean;
};

const VoteGlyph = () => (
  <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d="M20 6 9 17l-5-5" />
  </svg>
);
const CommentGlyph = () => (
  <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2Z" />
  </svg>
);

function ProposalCard({ p }: { p: GpProposal }) {
  const cat = CAT_META[p.category];
  const status = STATUSES.find((s) => s.id === p.status);
  return (
    <article className="gp__card">
      <div className="gp__cardtop">
        <span className="gp__avatar u-avatar" style={{ "--sz": "36px", "--hue": p.hue } as CSSProperties & { "--sz": string; "--hue": number }} aria-hidden="true" />
        <div className="gp__cardmain">
          <div className="gp__badges">
            {cat ? (
              <span className={"gp__pill gp__pill--" + cat.tone}>{cat.label}</span>
            ) : null}
            {status ? (
              <span className={"gp__status gp__status--" + status.tone}>{status.label}</span>
            ) : null}
          </div>
          <h2 className="gp__title">{p.title}</h2>
          <div className="gp__meta">
            <span className="gp__metaitem">by <b>{p.author}</b></span>
            {p.votes != null ? (
              <span className="gp__metaitem"><VoteGlyph /> {p.votes.toLocaleString()} votes</span>
            ) : null}
            {p.comments ? (
              <span className="gp__metaitem"><CommentGlyph /> {p.comments} comments</span>
            ) : null}
            <span className={"gp__metaitem" + (p.urgent ? " is-urgent" : "")}>{p.time}</span>
          </div>
        </div>
      </div>

      {p.forPct != null ? (
        <div className="gp__vote">
          <div className="gp__votebar" role="img" aria-label={`${p.forPct}% in favor, ${p.againstPct}% against`}>
            <span
              className={"gp__votefill" + (p.passing ? " is-pass" : " is-fail")}
              style={{ width: p.forPct + "%" }}
            />
          </div>
          <div className="gp__votelegend">
            <span className="gp__for">{p.forPct}% Yes</span>
            <span className="gp__against">{p.againstPct}% No</span>
          </div>
        </div>
      ) : null}
    </article>
  );
}

type GovernanceProposalsProps = {
  proposals?: GpProposal[];
  totalCount?: number;
  categoryCounts?: { [id: string]: number };
  pager?: ReactNode;
  initialQuery?: string;
  initialCategory?: string;
  initialStatus?: string;
};

export default function GovernanceProposals({
  proposals = [],
  totalCount,
  categoryCounts,
  pager = null,
  initialQuery = "",
  initialCategory = "all",
  initialStatus = "all",
}: GovernanceProposalsProps) {
  const [tab, setTab] = useState<GovernanceNavId>("proposals");
  const [category, setCategory] = useState(initialCategory);
  const [status, setStatus] = useState(initialStatus);
  const [timeframe, setTimeframe] = useState("all");
  const [sort, setSort] = useState("DESC");
  const [query, setQuery] = useState(initialQuery);

  const list = useMemo(() => {
    const sel = CATEGORIES.find((c) => c.id === category);
    let out = proposals.filter((p) => {
      if (category !== "all") {
        const ok = sel?.members ? sel.members.includes(p.category) : p.category === category;
        if (!ok) return false;
      }
      if (status !== "all" && p.status !== status) return false;
      if (query && !p.title.toLowerCase().includes(query.toLowerCase())) return false;
      return true;
    });
    if (sort === "ASC") out = [...out].reverse();
    return out;
  }, [proposals, category, status, query, sort]);

  const count = list.length;
  const localFilterActive =
    category !== initialCategory || status !== initialStatus || query.trim() !== initialQuery.trim();
  const displayCount =
    totalCount != null && !localFilterActive ? totalCount : count;

  return (
    <GovernanceChrome active={tab} onTab={setTab}>
      <div className="gp">
        <aside className="gp__filters" aria-label="Filters">
          <section className="gp__fsection">
            <h2 className="gp__ftitle">Category</h2>
            <ul className="gp__catlist">
              {CATEGORIES.map((c) => {
                const catCount = categoryCounts?.[c.id];
                return (
                <li key={c.id}>
                  <button
                    type="button"
                    className={"gp__catopt" + (c.id === category ? " is-active" : "")}
                    aria-pressed={c.id === category}
                    onClick={() => setCategory(c.id)}
                  >
                    <span className={"gp__catdot gp__catdot--" + c.tone} aria-hidden="true" />
                    <span className="gp__catlabel">{c.label}</span>
                    <span className="gp__catcount">{catCount != null ? catCount.toLocaleString() : "—"}</span>
                  </button>
                </li>
                );
              })}
            </ul>
          </section>

          <section className="gp__fsection">
            <h2 className="gp__ftitle">Status</h2>
            <div className="gp__chips">
              {STATUSES.map((s) => (
                <button
                  key={s.id}
                  type="button"
                  className={"gp__chip" + (s.id === status ? " is-active" : "")}
                  aria-pressed={s.id === status}
                  onClick={() => setStatus(s.id)}
                >
                  <span className={"gp__chipdot gp__chipdot--" + s.tone} aria-hidden="true" />
                  {s.label}
                </button>
              ))}
            </div>
          </section>

          <section className="gp__fsection">
            <h2 className="gp__ftitle">Time range</h2>
            <div className="gp__radios">
              {TIMEFRAMES.map((t) => (
                <label key={t.id} className="gp__radio">
                  <input
                    type="radio"
                    name="gp-timeframe"
                    checked={timeframe === t.id}
                    onChange={() => setTimeframe(t.id)}
                  />
                  <span className="gp__radiomark" aria-hidden="true" />
                  {t.label}
                </label>
              ))}
            </div>
          </section>

          <section className="gp__fsection">
            <h2 className="gp__ftitle">Sort by</h2>
            <div className="gp__radios">
              {SORTS.map((s) => (
                <label key={s.id} className="gp__radio">
                  <input
                    type="radio"
                    name="gp-sort"
                    checked={sort === s.id}
                    onChange={() => setSort(s.id)}
                  />
                  <span className="gp__radiomark" aria-hidden="true" />
                  {s.label}
                </label>
              ))}
            </div>
          </section>
        </aside>

        <section className="gp__main" aria-label="Proposals">
          <header className="gp__header">
            <div className="gp__heading">
              <h1 className="gp__h1">Proposals</h1>
              <p className="gp__sub">Browse, discuss and vote on the future of Decentraland.</p>
            </div>
            <a className="gp__submit" href="/governance/submit">Submit a proposal</a>
          </header>

          <div className="gp__toolbar">
            <div className="gp__search">
              <SearchField
                placeholder="Search proposals"
                value={query}
                onChange={setQuery}
              />
            </div>
            <div className="gp__count">
              {displayCount.toLocaleString()}{" "}
              {displayCount === 1 ? "proposal" : "proposals"}
            </div>
          </div>

          {count ? (
            <div className="gp__list">
              {list.map((p) => (
                <ProposalCard key={p.id} p={p} />
              ))}
            </div>
          ) : (
            <EmptyState
              className="gp__empty"
              iconWash
              icon={
                <svg viewBox="0 0 24 24" width="34" height="34" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
                  <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
                  <path d="M14 2v6h6M9 13h6M9 17h6" />
                </svg>
              }
              title="No proposals found"
              subtitle="Try a different category, status, or search term."
              actions={[{ label: "Submit a proposal", href: "/governance/submit", variant: "ghost" }]}
            />
          )}

          {pager}
        </section>
      </div>
    </GovernanceChrome>
  );
}
