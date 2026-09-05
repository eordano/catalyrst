import { useEffect, useRef } from "react";
import { Form, Link, useSearchParams } from "react-router";

import GvProjectsList from "@ui/governance/pages/GvProjectsList";

import type { ProjectCard, ProjectStats } from "@data/lib/catalyst/governance/projects";
import { track } from "@core/lib/telemetry/track";
import type { StoryId } from "@core/lib/telemetry/story-id";

const CATEGORY_OPTIONS = [
  { id: "", label: "All categories" },
  { id: "grants", label: "Grants" },
  { id: "bidding", label: "Bidding and tendering" },
];

const SUBTYPE_OPTIONS = [
  { id: "", label: "All grant subtypes" },
  { id: "accelerator", label: "Accelerator" },
  { id: "core_unit", label: "Core Unit" },
  { id: "documentation", label: "Documentation" },
  { id: "in_world_content", label: "In-World Content" },
  { id: "platform", label: "Platform" },
  { id: "social_media_content", label: "Social Media Content" },
  { id: "sponsorship", label: "Sponsorship" },
];

const STATUS_OPTIONS = [
  { id: "", label: "All outcomes" },
  { id: "ongoing", label: "Ongoing" },
  { id: "finished", label: "Finished" },
  { id: "paused", label: "Paused" },
];

const QUARTER_OPTIONS = [
  { id: "", label: "All quarters" },
  { id: "Q1", label: "Q1" },
  { id: "Q2", label: "Q2" },
  { id: "Q3", label: "Q3" },
  { id: "Q4", label: "Q4" },
];

const SORT_OPTIONS = [
  { id: "update_timestamp", label: "Last updated" },
  { id: "created_at", label: "Newest" },
  { id: "size", label: "Amount granted" },
];

const STORY: StoryId = "governance/projects";

export type ProjectsListProps = {
  sid: string;
  projects: ProjectCard[];
  stats: ProjectStats;
  years: number[];
  total: number;
  source: "live" | "unavailable";
  category: string;
  subtype: string;
  status: string;
  year: string;
  quarter: string;
  sort: string;
};

export default function ProjectsList({
  sid,
  projects,
  stats,
  years,
  total,
  source,
  category,
  subtype,
  status,
  year,
  quarter,
  sort,
}: ProjectsListProps) {
  const [, setSearchParams] = useSearchParams();

  const lastKey = useRef<string | null>(null);
  useEffect(() => {
    const key = `${category}|${subtype}|${status}|${year}|${quarter}|${sort}`;
    if (lastKey.current === key) return;
    const firstView = lastKey.current === null;
    lastKey.current = key;
    track(
      "gv_projects_viewed",
      {
        count: projects.length,
        total,
        source,
        category: category || null,
        subtype: subtype || null,
        status: status || null,
        year: year || null,
        quarter: quarter || null,
        sort: sort || null,
        grant_funding: stats.grantFunding,
        bid_funding: stats.bidFunding,
      },
      { sid, story: STORY },
    );
    if (!firstView) {
      track(
        "gv_projects_filtered",
        {
          category: category || null,
          subtype: subtype || null,
          status: status || null,
          year: year || null,
          quarter: quarter || null,
          sort: sort || null,
        },
        { sid, story: STORY },
      );
    }
  }, [
    sid,
    projects.length,
    total,
    source,
    category,
    subtype,
    status,
    year,
    quarter,
    sort,
    stats.grantFunding,
    stats.bidFunding,
  ]);

  function updateParam(key: string, value: string) {
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        if (value) next.set(key, value);
        else next.delete(key);
        if (key === "category" && value !== "grants") next.delete("subtype");
        if (key === "year" && !value) next.delete("quarter");
        return next;
      },
      { preventScrollReset: true },
    );
  }

  return (
    <div className="governance-projects-route">
      <Form method="get" replace style={BAR}>
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

        {category === "grants" && (
          <label style={LABEL}>
            Grant subtype
            <select
              name="subtype"
              defaultValue={subtype}
              onChange={(e) => updateParam("subtype", e.currentTarget.value)}
              style={CTRL}
            >
              {SUBTYPE_OPTIONS.map((o) => (
                <option key={o.id} value={o.id}>
                  {o.label}
                </option>
              ))}
            </select>
          </label>
        )}

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
          Year
          <select
            name="year"
            defaultValue={year}
            onChange={(e) => updateParam("year", e.currentTarget.value)}
            style={CTRL}
          >
            <option value="">All years</option>
            {years.map((y) => (
              <option key={y} value={String(y)}>
                {y}
              </option>
            ))}
          </select>
        </label>

        <label style={LABEL}>
          Quarter
          <select
            name="quarter"
            defaultValue={quarter}
            disabled={!year}
            onChange={(e) => updateParam("quarter", e.currentTarget.value)}
            style={CTRL}
          >
            {QUARTER_OPTIONS.map((o) => (
              <option key={o.id} value={o.id}>
                {o.label}
              </option>
            ))}
          </select>
        </label>

        <label style={LABEL}>
          Sort
          <select
            name="sort"
            defaultValue={sort}
            onChange={(e) => updateParam("sort", e.currentTarget.value)}
            style={CTRL}
          >
            {SORT_OPTIONS.map((o) => (
              <option key={o.id} value={o.id}>
                {o.label}
              </option>
            ))}
          </select>
        </label>

        <noscript>
          <button type="submit" style={CTRL}>
            Apply
          </button>
        </noscript>
      </Form>

      <GvProjectsList
        projects={
          projects as unknown as React.ComponentProps<typeof GvProjectsList>["projects"]
        }
      />

      <nav aria-label="Open a project" style={NAV}>
        {projects.length === 0 ? (
          <p style={{ color: "#736e7d", padding: "8px 0", margin: 0 }}>
            No projects match these filters.
          </p>
        ) : (
          projects.map((p) => (
            <Link
              key={p.id}
              to={`/governance/projects/${encodeURIComponent(p.id)}`}
              prefetch="intent"
              onClick={() =>
                track(
                  "gv_project_clicked",
                  {
                    project_id: p.id,
                    type: p.type,
                    category: p.category,
                    status: p.status,
                    size: p.size,
                  },
                  { sid, story: STORY },
                )
              }
              style={LINK}
            >
              <span>{p.title}</span>
              <span style={{ color: "#ff2d55", fontWeight: 600 }}>Open &#x2192;</span>
            </Link>
          ))
        )}
      </nav>
    </div>
  );
}

const BAR: React.CSSProperties = {
  display: "flex",
  gap: 12,
  flexWrap: "wrap",
  alignItems: "center",
  maxWidth: 1180,
  margin: "0 auto",
  padding: "16px 24px 0",
};

const NAV: React.CSSProperties = {
  display: "grid",
  gap: 8,
  maxWidth: 1180,
  margin: "0 auto",
  padding: "0 24px 48px",
};

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

const LINK: React.CSSProperties = {
  display: "flex",
  justifyContent: "space-between",
  alignItems: "center",
  padding: "10px 14px",
  borderRadius: 10,
  border: "1px solid rgba(0,0,0,.08)",
  background: "#fff",
  color: "#16141a",
  textDecoration: "none",
  fontWeight: 500,
};
