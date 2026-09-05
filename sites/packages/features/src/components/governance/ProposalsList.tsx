import { useEffect, useRef } from "react";
import { Form, Link, useSearchParams } from "react-router";

import GovernanceProposals from "@ui/governance/pages/GovernanceProposals";

import type { ProposalCard } from "@data/lib/catalyst/governance/index";
import { track } from "@core/lib/telemetry/track";
import type { StoryId } from "@core/lib/telemetry/story-id";

const CATEGORY_OPTIONS = [
  { id: "", label: "All categories" },
  { id: "poi", label: "Point of Interest" },
  { id: "catalyst", label: "Catalyst Node" },
  { id: "ban_name", label: "Name Ban" },
  { id: "grant", label: "Grant Request" },
  { id: "linked_wearables", label: "Linked Wearables" },
  { id: "governance", label: "Governance" },
  { id: "poll", label: "Poll" },
  { id: "tender", label: "Tender" },
];

const STATUS_OPTIONS = [
  { id: "", label: "All outcomes" },
  { id: "active", label: "Active" },
  { id: "passed", label: "Passed" },
  { id: "enacted", label: "Enacted" },
  { id: "rejected", label: "Rejected" },
  { id: "out_of_budget", label: "Out of Budget" },
];

export type ProposalsListProps = {
  sid: string;
  proposals: ProposalCard[];
  category: string;
  status: string;
  search: string;
  page: number;
  pageCount: number;
  totalFiltered: number;
  total: number;
  categoryCounts?: Record<string, number>;
  fallback?: boolean;
};

const STORY: StoryId = "governance/proposals";

const PROPOSALS_PATH = "/governance/proposals";

function pageWindow(page: number, pageCount: number): (number | "gap")[] {
  const out: (number | "gap")[] = [];
  const add = (n: number) => out.push(n);
  const pushGap = () => {
    if (out[out.length - 1] !== "gap") out.push("gap");
  };
  for (let p = 1; p <= pageCount; p++) {
    if (p === 1 || p === pageCount || (p >= page - 1 && p <= page + 1)) add(p);
    else pushGap();
  }
  return out;
}

export default function ProposalsList({
  sid,
  proposals,
  category,
  status,
  search,
  page,
  pageCount,
  totalFiltered,
  total,
  categoryCounts,
  fallback = false,
}: ProposalsListProps) {
  const [, setSearchParams] = useSearchParams();

  const lastKey = useRef<string | null>(null);
  useEffect(() => {
    const key = `${category}|${status}|${search}|${page}`;
    if (lastKey.current === key) return;
    const firstView = lastKey.current === null;
    lastKey.current = key;
    track(
      "gv_proposals_viewed",
      {
        count: proposals.length,
        total,
        totalFiltered,
        page,
        pageCount,
        category: category || null,
        status: status || null,
        search: search || null,
      },
      { sid, story: STORY },
    );
    if (!firstView) {
      track(
        "gv_proposals_filtered",
        { category: category || null, status: status || null, search: search || null },
        { sid, story: STORY },
      );
    }
  }, [sid, proposals.length, total, totalFiltered, page, pageCount, category, status, search]);

  function updateParam(key: string, value: string) {
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        if (value) next.set(key, value);
        else next.delete(key);
        next.delete("page");
        return next;
      },
      { preventScrollReset: true },
    );
  }

  function pageHref(p: number): string {
    const sp = new URLSearchParams();
    if (category) sp.set("category", category);
    if (status) sp.set("status", status);
    if (search) sp.set("search", search);
    if (p > 1) sp.set("page", String(p));
    const qs = sp.toString();
    return qs ? `${PROPOSALS_PATH}?${qs}` : PROPOSALS_PATH;
  }

  const pager =
    pageCount > 1 ? (
      <nav className="gp__pager" aria-label="Proposal pages">
        {page > 1 ? (
          <Link className="gp__pagerbtn" to={pageHref(page - 1)} prefetch="intent" rel="prev">
            &#x2190; Prev
          </Link>
        ) : (
          <span className="gp__pagerbtn is-disabled" aria-hidden="true">
            &#x2190; Prev
          </span>
        )}

        {pageWindow(page, pageCount).map((tok, i) =>
          tok === "gap" ? (
            <span key={`gap-${i}`} className="gp__pagerellipsis" aria-hidden="true">
              &#x2026;
            </span>
          ) : tok === page ? (
            <span key={tok} className="gp__pagerbtn is-active" aria-current="page">
              {tok}
            </span>
          ) : (
            <Link
              key={tok}
              className="gp__pagerbtn"
              to={pageHref(tok)}
              prefetch="intent"
              aria-label={`Page ${tok}`}
            >
              {tok}
            </Link>
          ),
        )}

        {page < pageCount ? (
          <Link className="gp__pagerbtn" to={pageHref(page + 1)} prefetch="intent" rel="next">
            Next &#x2192;
          </Link>
        ) : (
          <span className="gp__pagerbtn is-disabled" aria-hidden="true">
            Next &#x2192;
          </span>
        )}

        <p className="gp__pagerinfo">
          Page {page} of {pageCount} &#xB7; {totalFiltered.toLocaleString()}{" "}
          {totalFiltered === 1 ? "proposal" : "proposals"}
        </p>
      </nav>
    ) : null;

  return (
    <div className="governance-proposals-route">
      {fallback ? (
        <div
          role="status"
          style={{
            maxWidth: 1180,
            margin: "16px auto 0",
            padding: "10px 14px",
            borderRadius: 10,
            border: "1px solid rgba(180,120,0,.35)",
            background: "#fff7e6",
            color: "#7a4f00",
            fontSize: 13,
            fontWeight: 500,
          }}
        >
          Live governance data is unavailable right now. No proposals could be
          loaded &#x2014; please try again shortly.
        </div>
      ) : null}

      <Form
        method="get"
        replace
        style={{
          display: "flex",
          gap: 12,
          flexWrap: "wrap",
          alignItems: "center",
          maxWidth: 1180,
          margin: "0 auto",
          padding: "16px 24px 0",
        }}
      >
        <label style={LABEL}>
          Category
          <select
            name="category"
            defaultValue={category}
            onChange={(e) => updateParam("category", e.currentTarget.value)}
            style={CTRL}
          >
            {CATEGORY_OPTIONS.map((o) => (
              <option key={o.id} value={o.id}>
                {o.label}
              </option>
            ))}
          </select>
        </label>

        <label style={LABEL}>
          Status
          <select
            name="status"
            defaultValue={status}
            onChange={(e) => updateParam("status", e.currentTarget.value)}
            style={CTRL}
          >
            {STATUS_OPTIONS.map((o) => (
              <option key={o.id} value={o.id}>
                {o.label}
              </option>
            ))}
          </select>
        </label>

        <label style={LABEL}>
          Search
          <input
            type="search"
            name="search"
            defaultValue={search}
            placeholder="Search proposals"
            style={CTRL}
          />
        </label>

        <noscript>
          <button type="submit" style={CTRL}>
            Apply
          </button>
        </noscript>
      </Form>

      <GovernanceProposals
        proposals={
          proposals as unknown as React.ComponentProps<
            typeof GovernanceProposals
          >["proposals"]
        }
        totalCount={totalFiltered}
        categoryCounts={categoryCounts}
        initialQuery={search}
        initialCategory={category || "all"}
        initialStatus={status || "all"}
        pager={pager}
      />
    </div>
  );
}

const LABEL: React.CSSProperties = {
  display: "flex",
  flexDirection: "column",
  gap: 4,
  fontSize: 12,
  fontWeight: 600,
  color: "#16141a",
};

const CTRL: React.CSSProperties = {
  padding: "8px 10px",
  borderRadius: 8,
  border: "1px solid rgba(0,0,0,.18)",
  background: "#fff",
  color: "#16141a",
  fontSize: 14,
};
