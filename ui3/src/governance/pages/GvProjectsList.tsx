import { useMemo, useState, type CSSProperties } from "react";
import GovernanceChrome, { type GovernanceNavId } from "../frames/GovernanceChrome";
import "./gvprojectslist.css";
import { Caret } from "../../atoms/icons";

type VarStyle = CSSProperties & { [k: `--${string}`]: string | number };
const cssVars = (s: VarStyle): CSSProperties => s;

type ProjectUpdate = { index: number; intro: string; date: string; late: boolean };
type Project = {
  id: string;
  title: string;
  type: string;
  category: string;
  status: string;
  author: string;
  hue: number;
  size: number;
  vested: number;
  released: number;
  total: number;
  vestedPct: number;
  releasedPct: number;
  started: string;
  ends: string;
  token: string;
  update: ProjectUpdate;
};

const GRANT_SUBTYPES = [
  { id: "all", label: "All" },
  { id: "accelerator", label: "Accelerator" },
  { id: "core_unit", label: "Core Unit" },
  { id: "documentation", label: "Documentation" },
  { id: "in_world_content", label: "In-World Content" },
  { id: "platform", label: "Platform" },
  { id: "social_media_content", label: "Social Media Content" },
  { id: "sponsorship", label: "Sponsorship" },
];

const STATUSES = [
  { id: "all", label: "All outcomes" },
  { id: "pending", label: "Pending" },
  { id: "in_progress", label: "Ongoing" },
  { id: "finished", label: "Finished" },
  { id: "paused", label: "Paused" },
  { id: "revoked", label: "Revoked" },
];

const YEARS = [2022, 2023, 2024, 2025, 2026];
const QUARTERS = [1, 2, 3, 4];

const SORTS = [
  { id: "update_timestamp", label: "Last updated" },
  { id: "created_at", label: "Newest" },
  { id: "size", label: "Amount granted" },
];

const CATEGORY_TONE: Record<string, string> = {
  accelerator: "green",
  core_unit: "blue",
  documentation: "purple",
  in_world_content: "red",
  platform: "fuchsia",
  social_media_content: "yellow",
  sponsorship: "orange",
  tender: "red",
};
const CATEGORY_PILL_LABEL: Record<string, string> = {
  accelerator: "Accelerator",
  core_unit: "Core",
  documentation: "Documentation",
  in_world_content: "In-World",
  platform: "Platform",
  social_media_content: "Social",
  sponsorship: "Sponsorship",
  tender: "Tender",
};


const usd = (n: number) => "$" + Math.round(n).toLocaleString("en-US");
const num = (n: number) => Math.round(n).toLocaleString("en-US");

const ChevronRight = ({ color = "currentColor" }: { color?: string }) => (
  <svg viewBox="0 0 24 24" width="16" height="16" fill="none" aria-hidden="true">
    <path d="M9 6l6 6-6 6" stroke={color} strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
  </svg>
);
const SubItem = () => (
  <svg viewBox="0 0 16 16" width="13" height="13" fill="none" aria-hidden="true">
    <path d="M4 3v5a3 3 0 0 0 3 3h5" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" />
  </svg>
);
const Watermelon = () => (
  <svg viewBox="0 0 48 48" width="44" height="44" fill="none" aria-hidden="true">
    <path d="M6 12a18 18 0 0 0 36 0Z" fill="#44b600" />
    <path d="M9 13a15 15 0 0 0 30 0Z" fill="#fff" opacity=".55" />
    <path d="M11 13.5a13 13 0 0 0 26 0Z" fill="#ff5f87" />
    <circle cx="19" cy="22" r="1.3" fill="#161518" />
    <circle cx="24" cy="25" r="1.3" fill="#161518" />
    <circle cx="29" cy="22" r="1.3" fill="#161518" />
  </svg>
);
const UpdateCheck = ({ missed }: { missed: boolean }) => (
  <svg viewBox="0 0 25 25" width="25" height="25" aria-hidden="true">
    <circle cx="12.5" cy="12.5" r="11" fill={missed ? "#dddce0" : "rgba(68,182,0,.16)"} />
    <path
      d="M8 12.8l3 3 6-6.4"
      fill="none"
      stroke={missed ? "#736e7d" : "#44b600"}
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
    />
  </svg>
);

function ProjectCard({ p }: { p: Project }) {
  const tone = CATEGORY_TONE[p.category] || "gray";
  const pillLabel = CATEGORY_PILL_LABEL[p.category] || p.category;
  const oneTime = p.vestedPct >= 100 && p.releasedPct >= 100;
  return (
    <a className="gvpl__card" href={`/governance/projects/${p.id}`}>
      <div className="gvpl__cardbody">
        <div className="gvpl__cardhead">
          <div className="gvpl__cfg">
            <span className={"gvpl__pill gvpl__pill--" + tone}>{pillLabel}</span>
            <span className="gvpl__size">{`Size: ${usd(p.size)} USD`}</span>
          </div>
          <div className="gvpl__by">
            By
            <span className="gvpl__byavatar u-avatar" style={cssVars({ "--sz": "16px", "--hue": p.hue })} aria-hidden="true" />
            <span className="gvpl__byaddr">{p.author}</span>
          </div>
        </div>

        <div className="gvpl__headline">
          <h3 className="gvpl__title">{p.title}</h3>
          <span className="gvpl__avatar u-avatar" style={cssVars({ "--sz": "44px", "--hue": p.hue })} aria-hidden="true" />
        </div>

        <div className="gvpl__vesting">
          <div className="gvpl__vlabels">
            <div className="gvpl__vested">
              <span className="gvpl__vbold gvpl__ellipsis">{`${num(p.vested)} ${p.token}`}</span>
              <span className="gvpl__ellipsis">vested</span>
              <span className={"gvpl__pct gvpl__pct--" + (oneTime ? "fuchsia" : "yellow")}>{p.vestedPct}%</span>
            </div>
            <div className="gvpl__released gvpl__ellipsis">
              <span className="gvpl__rdot" aria-hidden="true" />
              <span className="gvpl__ellipsis">{`${num(p.released)} ${p.token} released`}</span>
            </div>
          </div>
          <div className="gvpl__bar">
            {p.releasedPct > 0 && (
              <span className="gvpl__bar-released" style={{ width: p.releasedPct + "%" }} />
            )}
            {p.vestedPct > 0 && <span className="gvpl__bar-vested" style={{ width: p.vestedPct + "%" }} />}
          </div>
          <div className="gvpl__dates">
            <div className="gvpl__date">
              <span>Started</span>
              <span className="gvpl__datev">{p.started}</span>
            </div>
            <div className="gvpl__date">
              <span>{p.ends.startsWith("ended") ? "Ended" : "Ends"}</span>
              <span className="gvpl__datev">{p.ends.replace(/^ended /, "")}</span>
            </div>
          </div>
        </div>
      </div>

      <div className="gvpl__update">
        <div className="gvpl__uleft">
          <span className="gvpl__uicon">
            <UpdateCheck missed={false} />
          </span>
          <div className="gvpl__udesc">
            <span className="gvpl__uindex">{`Update #${p.update.index}:`}</span>
            <span className="gvpl__uintro">{p.update.intro}</span>
          </div>
        </div>
        <div className="gvpl__udate">
          <span className="gvpl__udatetext">{p.update.date}</span>
          {p.update.late && <span className="gvpl__ulate">Late</span>}
          <ChevronRight color="var(--gv-faint)" />
        </div>
      </div>
    </a>
  );
}

function MetricsCard({
  category,
  title,
  description,
  href,
}: {
  category: string;
  title: string;
  description?: string;
  href?: string;
}) {
  const inner = (
    <>
      <p className="gvpl__metric-cat">{category}</p>
      <h2 className="gvpl__metric-title">{title}</h2>
      {description && <p className="gvpl__metric-desc">{description}</p>}
    </>
  );
  return href ? (
    <a className="gvpl__metric" href={href}>{inner}</a>
  ) : (
    <div className="gvpl__metric">{inner}</div>
  );
}

export default function GvProjectsList({ projects = [] }: { projects?: Project[] }) {
  const [tab, setTab] = useState<GovernanceNavId>("projects");
  const [type, setType] = useState("");
  const [subtype, setSubtype] = useState("");
  const [status, setStatus] = useState("all");
  const [year, setYear] = useState("");
  const [quarter, setQuarter] = useState<number | "">("");
  const [sort, setSort] = useState("update_timestamp");
  const [sortOpen, setSortOpen] = useState(false);
  const [grantsOpen, setGrantsOpen] = useState(true);
  const [bannerOpen, setBannerOpen] = useState(true);

  const counter = useMemo(
    () => ({
      grants: projects.filter((p) => p.type === "grant").length,
      bidding_and_tendering: projects.filter((p) => p.type === "bid").length,
    }),
    [projects]
  );

  const displayable = useMemo(() => {
    let out = projects.slice();
    if (type === "bidding_and_tendering") out = out.filter((p) => p.type === "bid");
    else if (type === "grants") {
      out = out.filter((p) => p.type === "grant");
      if (subtype && subtype !== "all") out = out.filter((p) => p.category === subtype);
    }
    if (status !== "all") out = out.filter((p) => p.status === status);
    return out;
  }, [projects, type, subtype, status]);

  const sorted = useMemo(() => {
    const out = displayable.slice();
    if (sort === "size") out.sort((a, b) => b.size - a.size);
    return out;
  }, [displayable, sort]);

  const statusLabel = status !== "all" ? STATUSES.find((s) => s.id === status)?.label + " " : "";
  const categoryLabel =
    type === "grants"
      ? subtype && subtype !== "all"
        ? GRANT_SUBTYPES.find((g) => g.id === subtype)?.label
        : "Grants"
      : type === "bidding_and_tendering"
      ? "Bidding and tendering"
      : "All";
  const timeframeLabel = year && quarter ? `Q${quarter} ${year} ` : year ? `${year} ` : "";

  const isBT = type === "bidding_and_tendering";
  const grantFunding = displayable.filter((p) => p.type === "grant").reduce((t, p) => t + p.size, 0);
  const bidFunding = displayable.filter((p) => p.type === "bid").reduce((t, p) => t + p.size, 0);

  return (
    <GovernanceChrome active={tab} onTab={setTab}>
      <div className="gvpl">
        {bannerOpen && (
        <section className="gvpl__banner">
          <div className="gvpl__banner-bg" aria-hidden="true" />
          <div className="gvpl__banner-text">
            <h2 className="gvpl__banner-title">Initiatives making the Decentraland Ecosystem grow</h2>
            <p className="gvpl__banner-desc">
              Every quarter, the DAO defines Treasury funding allocation and distribution to community-led projects.
              Keep browsing to see their goals, progress, and how they are making an impact in our project.
            </p>
          </div>
          <div className="gvpl__banner-items">
            {[
              {
                title: "Budget Definition",
                desc: "Review Quarterly Governance Proposal",
                href: "/governance/proposals/bfab7b70-7b75-11ed-ad27-015f26e7c35c",
              },
              {
                title: "Transparency",
                desc: "How the Grants System is performing",
                href: "/governance/transparency",
              },
              {
                title: "FAQ",
                desc: "Learn more about our Grants System",
                href: "/docs/player/dao/the-dao-fund/",
              },
            ].map((it, i, arr) => (
              <div className="gvpl__bitemwrap" key={it.title}>
                <a className="gvpl__bitem" href={it.href}>
                  <div>
                    <h3 className="gvpl__bitem-title">{it.title}</h3>
                    <p className="gvpl__bitem-desc">{it.desc}</p>
                  </div>
                  <ChevronRight color="#fff" />
                </a>
                {i !== arr.length - 1 && <div className="gvpl__bitem-divider" />}
              </div>
            ))}
          </div>
          <button
            type="button"
            className="gvpl__banner-collapse"
            aria-label="Hide projects banner"
            onClick={() => setBannerOpen(false)}
          >
            <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
              <path d="M5 12h14" stroke="#fff" strokeWidth="2" strokeLinecap="round" />
            </svg>
          </button>
        </section>
        )}

        <div className="gvpl__container">
          <aside className="gvpl__sidebar" aria-label="Filters">
            <div className="gvpl__fc">
              <div className="gvpl__fc-title">Filter by Category</div>
              <div className="gvpl__fc-content">
                <button
                  type="button"
                  className={"gvpl__cat" + (type === "grants" ? " is-active" : "")}
                  onClick={() => {
                    setGrantsOpen((o) => (type === "grants" ? !o : true));
                    setType("grants");
                    setSubtype("");
                  }}
                >
                  <span className="gvpl__cat-titlewrap">
                    <span className="gvpl__cat-left">
                      <span className="gvpl__cat-icon gvpl__cat-icon--grant" aria-hidden="true" />
                      <span className="gvpl__cat-label">Grants</span>
                    </span>
                    <span className={"gvpl__cat-arrow" + (grantsOpen ? " is-open" : "")}>
                      <Caret size={11} />
                    </span>
                  </span>
                  <span className={"gvpl__cat-count" + (type === "grants" ? " is-active" : "")}>{counter.grants}</span>
                </button>
                <div className={"gvpl__subcats gvpl__subcats--grant" + (grantsOpen ? " is-open" : "")}>
                  {GRANT_SUBTYPES.map((g) => {
                    const active = type === "grants" && (subtype || "all") === g.id;
                    return (
                      <button
                        key={g.id}
                        type="button"
                        className={"gvpl__subcat" + (active ? " is-active" : "")}
                        onClick={(e) => {
                          e.preventDefault();
                          setType("grants");
                          setSubtype(g.id === "all" ? "" : g.id);
                        }}
                      >
                        <SubItem />
                        {g.label}
                      </button>
                    );
                  })}
                </div>

                <button
                  type="button"
                  className={"gvpl__cat" + (type === "bidding_and_tendering" ? " is-active" : "")}
                  onClick={() => {
                    setType("bidding_and_tendering");
                    setSubtype("");
                  }}
                >
                  <span className="gvpl__cat-titlewrap">
                    <span className="gvpl__cat-left">
                      <span className="gvpl__cat-icon gvpl__cat-icon--bt" aria-hidden="true" />
                      <span className="gvpl__cat-label">Bidding and tendering</span>
                    </span>
                  </span>
                  <span className={"gvpl__cat-count" + (type === "bidding_and_tendering" ? " is-active" : "")}>
                    {counter.bidding_and_tendering}
                  </span>
                </button>
              </div>
            </div>

            <div className="gvpl__fc">
              <div className="gvpl__fc-title">Filter by Status</div>
              <div className="gvpl__fc-content">
                {STATUSES.map((s) => (
                  <button
                    key={s.id}
                    type="button"
                    className={"gvpl__flabel" + (status === s.id ? " is-active" : "")}
                    onClick={(e) => {
                      e.preventDefault();
                      setStatus(s.id);
                    }}
                  >
                    <span className="gvpl__flabel-text">{s.label}</span>
                  </button>
                ))}
              </div>
            </div>

            <div className="gvpl__fc">
              <div className="gvpl__fc-title">Filter by TimeFrame</div>
              <div className="gvpl__quarter">
                <div className="gvpl__year">
                  <select
                    className="gvpl__yearsel"
                    aria-label="Filter by year"
                    value={year}
                    onChange={(e) => {
                      setYear(e.target.value);
                      setQuarter("");
                    }}
                  >
                    <option value="">Choose a timeframe</option>
                    {YEARS.map((y) => (
                      <option key={y} value={y}>
                        {y}
                      </option>
                    ))}
                  </select>
                  <span className="gvpl__yearcaret" aria-hidden="true">
                    <Caret size={11} />
                  </span>
                </div>
                {year && (
                  <div className="gvpl__quarters">
                    {QUARTERS.map((q) => (
                      <button
                        key={q}
                        type="button"
                        className={"gvpl__qbtn" + (String(quarter) === String(q) ? " is-active" : "")}
                        onClick={() => setQuarter(String(quarter) === String(q) ? "" : q)}
                      >
                        Q{q}
                      </button>
                    ))}
                  </div>
                )}
              </div>
            </div>

            <div className="gvpl__request">
              <div className="gvpl__request-title">Get funded</div>
              <div className="gvpl__request-desc">Decentraland could be the means to realize that project of yours</div>
              <a
                className="gvpl__request-btn"
                href="/governance/submit/grant"
                style={{ display: "flex", alignItems: "center", justifyContent: "center", textDecoration: "none" }}
              >
                Request a Grant
              </a>
            </div>
          </aside>

          <section className="gvpl__list" aria-label="Projects">
            <div className="gvpl__list-titlerow">
              <h2 className="gvpl__list-title">
                {timeframeLabel}
                {statusLabel}
                {categoryLabel} Projects
              </h2>
              <div className="gvpl__sort">
                <button type="button" className="gvpl__sortbtn" onClick={() => setSortOpen((o) => !o)}>
                  {SORTS.find((s) => s.id === sort)?.label}
                  <Caret size={11} />
                </button>
                {sortOpen && (
                  <ul className="gvpl__sortmenu">
                    {SORTS.map((s) => (
                      <li key={s.id}>
                        <button
                          type="button"
                          className={"gvpl__sortitem" + (s.id === sort ? " is-active" : "")}
                          onClick={() => {
                            setSort(s.id);
                            setSortOpen(false);
                          }}
                        >
                          {s.label}
                        </button>
                      </li>
                    ))}
                  </ul>
                )}
              </div>
            </div>

            <div className="gvpl__stats">
              {isBT ? (
                <>
                  <MetricsCard category="Project funding" title={usd(bidFunding)} />
                  <MetricsCard category="opportunities" title="3 Tender submissions open" href="/governance/proposals" />
                  <MetricsCard category="opportunities" title="5 Bid submissions open" href="/governance/proposals" />
                </>
              ) : (
                <>
                  <MetricsCard
                    category="Ongoing"
                    title={`${displayable.filter((p) => p.status !== "finished" && p.status !== "revoked").length} projects`}
                  />
                  <MetricsCard
                    category="Project funding"
                    title={usd(grantFunding + bidFunding)}
                    description={`Grants: ${usd(grantFunding)}; B&T: ${usd(bidFunding)}`}
                  />
                  <MetricsCard category="opportunities" title="5 Bid submissions open" href="/governance/proposals" />
                </>
              )}
            </div>

            {sorted.length > 0 ? (
              <div className="gvpl__grid">
                {sorted.map((p) => (
                  <ProjectCard key={p.id} p={p} />
                ))}
              </div>
            ) : (
              <div className="gvpl__empty">
                <div className="gvpl__empty-icon" aria-hidden="true">
                  <Watermelon />
                </div>
                <p className="gvpl__empty-desc">
                  Looks like there are no Projects following these criteria to be displayed
                </p>
                <button
                  type="button"
                  className="gvpl__empty-btn"
                  onClick={() => {
                    setType("");
                    setSubtype("");
                    setStatus("all");
                    setYear("");
                    setQuarter("");
                  }}
                >
                  View all Projects
                </button>
              </div>
            )}
          </section>
        </div>
      </div>
    </GovernanceChrome>
  );
}
