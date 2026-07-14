import type { CSSProperties, ReactNode } from "react";
import { useMemo, useState } from "react";
import GovernanceChrome, { type GovernanceNavId } from "../frames/GovernanceChrome";
import "./gvhomelanding.css";

type Metric = { category: string; title: string; description: string };
export type GvHomeMetric = Metric;

export type GvHomeStat = { value: string; label: string };

export type GvHomeProposal = {
  id: number | string;
  title: string;
  author: string;
  hue: number;
  type: string;
  tone: string;
  votes?: number;
  comments?: number;
  time: string;
  urgent?: boolean;
  met: boolean;
  vp: string;
};
type HomeProposal = GvHomeProposal;

export type GvHomeGrant = {
  id: number;
  title: string;
  category: string;
  hue: number;
  size: string;
  pct: number;
  months: number;
  update: string;
};
type Grant = GvHomeGrant;

export type GvHomeDelegate = {
  address: string;
  name: string;
  hue: number;
  lastVoted: string;
  timesVoted: number;
  pickedBy: number;
  totalVP: string;
};
type Delegate = GvHomeDelegate;

export type GvHomeTopVoter = { rank: number; name: string; hue: number; votes: number };
type TopVoter = GvHomeTopVoter;

export type GvHomeChartPoint = { label: string; value: number };

export type GvHomeActivity = { id: number; kind: string; hue?: number; html: string; date: string };
type HomeActivity = GvHomeActivity;

const Chevron = () => (
  <svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">
    <path d="M6 4l4 4-4 4" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" fill="none" />
  </svg>
);
const SortGlyph = () => (
  <svg viewBox="0 0 12 12" width="10" height="10" aria-hidden="true" className="gh__sort">
    <path d="M6 1l3 3H3zM6 11l3-3H3z" fill="currentColor" />
  </svg>
);
const CommentBubble = () => (
  <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2Z" />
  </svg>
);

function MainBanner({ onClose }: { onClose?: () => void }) {
  return (
    <div className="gh__banner">
      <div className="gh__bannerbg" aria-hidden="true" />
      <h2 className="gh__bannertitle">Get involved and shape Decentraland&#x2019;s future.</h2>
      <p className="gh__bannerdesc">
        The DAO is a key component of the ecosystem and it is the consensus mechanism for defining
        the rules of the Decentraland&#x2019;s virtual world. Participate in the DAO and make your voice heard.
      </p>
      <div className="gh__bannerbtns">
        <a className="gh__bannerbtn gh__bannerbtn--discord" href="https://dcl.gg/discord" target="_blank" rel="noopener noreferrer">Join our Discord</a>
        <a className="gh__bannerbtn gh__bannerbtn--docs" href="/docs/">Read our docs</a>
      </div>
      <button type="button" className="gh__bannerclose" aria-label="Dismiss banner" onClick={onClose}>
        <svg viewBox="0 0 24 24" width="22" height="22" aria-hidden="true">
          <circle cx="12" cy="12" r="11" fill="rgba(255,255,255,.22)" />
          <path d="M8.5 8.5l7 7M15.5 8.5l-7 7" stroke="#fff" strokeWidth="1.8" strokeLinecap="round" />
        </svg>
      </button>
    </div>
  );
}

function MetricsCard({ category, title, description }: Metric) {
  return (
    <div className="gh__metric">
      <p className="gh__metriccat">{category}</p>
      <h2 className="gh__metrictitle">{title}</h2>
      <p className="gh__metricdesc">{description}</p>
    </div>
  );
}

function SectionHeader({ title, description }: { title: ReactNode; description: ReactNode }) {
  return (
    <>
      <h2 className="gh__sectiontitle">{title}</h2>
      <p className="gh__sectiondesc">{description}</p>
    </>
  );
}

function ProposalPreviewCard({ p }: { p: HomeProposal }) {
  return (
    <a className="gh__pcard" href={`/governance/proposals/${p.id}`}>
      <span className="gh__psection">
        <span className="gh__pavatar u-avatar" style={{ "--sz": "36px", "--hue": p.hue } as CSSProperties & { "--sz": string; "--hue": number }} aria-hidden="true" />
        <span className="gh__ptext">
          <h3 className="gh__ptitle">{p.title}</h3>
          <span className="gh__pdetails">
            <span className="gh__pdetail">By <b>{p.author}</b></span>
            {p.votes != null && <span className="gh__pdetail">{p.votes} votes</span>}
            {p.comments != null && <span className="gh__pdetail">{p.comments} comments</span>}
            <span className={"gh__pdetail" + (p.urgent ? " is-urgent" : "")}>{p.time}</span>
          </span>
        </span>
      </span>
      <span className="gh__pvote">
        <span className="gh__ppillwrap">
          <span className={"gh__cpill gh__cpill--" + p.tone}>{p.type}</span>
        </span>
        <span className="gh__pvotingwrap">
          <span className="gh__pconsensus">{p.met ? "Threshold met" : "Threshold still not met"}</span>
          <span className="gh__pvpneeded">{p.vp} VP{p.met ? "" : " needed"}</span>
        </span>
        <span className="gh__pvotecta">
          <span className="gh__pvotetext">Vote</span>
          <span className="gh__pchev"><Chevron /></span>
        </span>
      </span>
    </a>
  );
}

function ProjectCard({ g }: { g: Grant }) {
  return (
    <a className="gh__gcard" href={`/governance/projects/${g.id}`}>
      <div className="gh__gtop">
        <div className="gh__ghead">
          <span className="gh__gavatar u-avatar" style={{ "--sz": "30px", "--hue": g.hue } as CSSProperties & { "--sz": string; "--hue": number }} aria-hidden="true" />
          <span className="gh__gcat">{g.category}</span>
        </div>
        <h3 className="gh__gtitle">{g.title}</h3>
        <div className="gh__gfunding">
          <span className="gh__gsize">{g.size}</span>
          <span className="gh__gmonths">{g.months} months vesting</span>
        </div>
        <div className="gh__gbar" role="img" aria-label={`${g.pct}% vested`}>
          <span className="gh__gfill" style={{ width: g.pct + "%" }} />
        </div>
      </div>
      <div className="gh__gupdate">
        <span className="gh__gupdateicon"><CommentBubble /></span>
        <span className="gh__gupdatetext">{g.update}</span>
      </div>
    </a>
  );
}

type BoxTab = { id: string; label: ReactNode };

function BoxTabs({ tabs, active, onSelect, right }: { tabs: BoxTab[]; active: string; onSelect: (id: string) => void; right?: ReactNode }) {
  return (
    <div className="gh__boxtabs">
      <div className="gh__boxtabsleft">
        {tabs.map((tb) => (
          <button
            key={tb.id}
            type="button"
            className={"gh__boxtab" + (tb.id === active ? " is-active" : "")}
            onClick={() => onSelect(tb.id)}
          >
            {tb.label}
          </button>
        ))}
      </div>
      {right ? <div className="gh__boxtabsright">{right}</div> : null}
    </div>
  );
}

function LineChart({ points }: { points: GvHomeChartPoint[] }) {
  const w = 560;
  const h = 220;
  const pad = 16;
  const values = points.map((p) => p.value);
  const max = Math.max(...values);
  const min = Math.min(...values);
  const stepX = (w - pad * 2) / (values.length - 1);
  const pts = values.map((v, i): [number, number] => {
    const x = pad + i * stepX;
    const y = pad + (h - pad * 2) * (1 - (v - min) / (max - min || 1));
    return [x, y];
  });
  const line = pts.map(([x, y], i) => (i === 0 ? `M${x},${y}` : `L${x},${y}`)).join(" ");
  const first = pts[0];
  const last = pts[pts.length - 1];
  const area = first && last ? `${line} L${last[0]},${h - pad} L${first[0]},${h - pad} Z` : line;
  return (
    <div className="gh__chart">
      <svg viewBox={`0 0 ${w} ${h + 22}`} preserveAspectRatio="none" className="gh__chartsvg" role="img" aria-label="Participating VP over time">
        <defs>
          <linearGradient id="ghChartFill" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="#ff2d55" stopOpacity="0.22" />
            <stop offset="100%" stopColor="#ff2d55" stopOpacity="0" />
          </linearGradient>
          <linearGradient id="ghChartLine" x1="0" y1="0" x2="1" y2="0">
            <stop offset="0%" stopColor="#ff2d55" />
            <stop offset="100%" stopColor="#c640cd" />
          </linearGradient>
        </defs>
        <path d={area} fill="url(#ghChartFill)" />
        <path d={line} fill="none" stroke="url(#ghChartLine)" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round" />
        {pts.map(([x, y], i) => (
          <circle key={i} cx={x} cy={y} r="3" fill="#ff2d55" />
        ))}
        {points.map((p, i) => (
          <text key={p.label + i} x={pad + i * stepX} y={h + 14} textAnchor="middle" className="gh__chartlabel">{p.label}</text>
        ))}
      </svg>
    </div>
  );
}

function ActivityItem({ item }: { item: HomeActivity }) {
  return (
    <div className="gh__tickeritem">
      {item.kind === "comment" || item.kind === "finished" || item.kind === "vesting" ? (
        <span className="gh__tickerglyph" aria-hidden="true"><CommentBubble /></span>
      ) : (
        <span className="gh__tickeravatar u-avatar" style={{ "--sz": "28px", "--hue": item.hue } as CSSProperties & { "--sz": string; "--hue": number }} aria-hidden="true" />
      )}
      <div className="gh__tickerbody">
        <p className="gh__tickertext" dangerouslySetInnerHTML={{ __html: item.html }} />
        <span className="gh__tickerdate">{item.date}</span>
      </div>
    </div>
  );
}

const TICKER_KIND_LABELS: Record<string, string> = {
  vote: "Votes",
  comment: "Comments",
  proposal: "Proposals",
  delegation: "Delegations",
  finished: "Voting finished",
  update: "Project updates",
  vesting: "Vesting",
};

const OPEN_TABS: BoxTab[] = [
  { id: "endingSoon", label: "Ending soon" },
  { id: "participatingVp", label: "Participating VP" },
];
const CHART_TABS: BoxTab[] = [
  { id: "vp", label: "Participating VP over time" },
  { id: "votes", label: "Votes per proposal" },
];

type GvHomeLandingProps = {
  metrics?: Metric[];
  endingSoon?: HomeProposal[];
  grants?: Grant[];
  delegates?: Delegate[];
  topVoters?: TopVoter[];
  activity?: HomeActivity[];
  activityUnavailable?: boolean;
  chartPoints?: GvHomeChartPoint[];
  engagementUnavailable?: boolean;
  bottomStats?: GvHomeStat[];
};

export default function GvHomeLanding({
  metrics = [],
  endingSoon = [],
  grants = [],
  delegates = [],
  topVoters = [],
  activity = [],
  activityUnavailable = false,
  chartPoints = [],
  engagementUnavailable = false,
  bottomStats = [],
}: GvHomeLandingProps) {
  const [chromeTab, setChromeTab] = useState<GovernanceNavId>("home");
  const [showBanner, setShowBanner] = useState(true);
  const [openTab, setOpenTab] = useState("endingSoon");
  const [chartTab, setChartTab] = useState("vp");
  const [sortKey, setSortKey] = useState("totalVP");
  const [tickerKind, setTickerKind] = useState("all");
  const [tickerFilterOpen, setTickerFilterOpen] = useState(false);

  const openList = useMemo(
    () => (openTab === "endingSoon" ? endingSoon : [...endingSoon].slice(0, 3)),
    [openTab, endingSoon]
  );

  const tickerKinds = useMemo(
    () => [...new Set(activity.map((a) => a.kind))],
    [activity]
  );
  const tickerItems = useMemo(
    () => (tickerKind === "all" ? activity : activity.filter((a) => a.kind === tickerKind)),
    [activity, tickerKind]
  );

  return (
    <GovernanceChrome active={chromeTab} onTab={setChromeTab}>
      <div className="gh">
        <div className="gh__container">
          <div className="gh__content">
            {showBanner && <MainBanner onClose={() => setShowBanner(false)} />}

            {metrics.length > 0 && (
              <div className="gh__metrics">
                {metrics.map((m) => (
                  <MetricsCard key={m.category} {...m} />
                ))}
              </div>
            )}

            <section className="gh__section">
              <SectionHeader
                title="Open Proposals"
                description="Proposals are created by the community and work as the consensus mechanism used to outline policies and changes to the Decentraland ecosystem."
              />
              <div className="gh__boxtabscontainer">
                <BoxTabs tabs={OPEN_TABS} active={openTab} onSelect={setOpenTab} />
                {openList.length ? (
                  openList.map((p) => <ProposalPreviewCard key={p.id} p={p} />)
                ) : (
                  <div className="gh__boxempty">No active proposals</div>
                )}
              </div>
              <a className="gh__fullbtn" href="/governance/proposals">View all proposals</a>
            </section>

            {endingSoon.length > 0 && (
              <section className="gh__section">
                <SectionHeader
                  title="Priority Proposals Spotlight"
                  description="Your vote matters. This section highlights binding Governance and project-related proposals that need your attention now."
                />
                <div className="gh__spotlight">
                  {endingSoon.slice(0, 2).map((p) => (
                    <a key={p.id} className="gh__slim" href={`/governance/proposals/${p.id}`}>
                      <span className={"gh__cpill gh__cpill--" + p.tone}>{p.type}</span>
                      <span className="gh__slimtitle">{p.title}</span>
                      <span className="gh__slimchev"><Chevron /></span>
                    </a>
                  ))}
                </div>
              </section>
            )}

            <section className="gh__section">
              <SectionHeader
                title="Active Community Grants"
                description="Grants are the mechanism that the DAO uses to fund community projects that add value to the Decentraland ecosystem with our Treasury."
              />
              <div className="gh__grants">
                {grants.map((g) => (
                  <ProjectCard key={g.id} g={g} />
                ))}
              </div>
              <a className="gh__fullbtn" href="/governance/projects">View all Grants</a>
            </section>

            <section className="gh__section">
              <SectionHeader
                title="DAO Delegates"
                description="Voting Power (VP) delegation is crucial to maximize the circulating VP usage, by assigning your VP to recognized community members who can engage in the governance process on behalf of you."
              />
              <div className="gh__delwrap">
                <table className="gh__deltable">
                  <thead>
                    <tr>
                      <th className="gh__delname">Candidate Name</th>
                      {[
                        { k: "lastVoted", l: "Last Voted" },
                        { k: "timesVoted", l: "Voted On" },
                        { k: "pickedBy", l: "Picked By" },
                        { k: "totalVP", l: "Total VP" },
                      ].map((c) => (
                        <th
                          key={c.k}
                          className={"gh__delsort" + (sortKey === c.k ? " is-active" : "")}
                          onClick={() => setSortKey(c.k)}
                        >
                          <span>{c.l}<SortGlyph /></span>
                        </th>
                      ))}
                      <th className="gh__delarrow"><span className="u-sr-only">Open delegate</span></th>
                    </tr>
                  </thead>
                  <tbody>
                    {delegates.map((d) => (
                      <tr key={d.address} className="gh__delrow">
                        <td className="gh__delnamecell">
                          <span className="gh__delavatar u-avatar" style={{ "--sz": "28px", "--hue": d.hue } as CSSProperties & { "--sz": string; "--hue": number }} aria-hidden="true" />
                          <span className="gh__delusername">{d.name}</span>
                        </td>
                        <td>{d.lastVoted}</td>
                        <td>{d.timesVoted}</td>
                        <td>{d.pickedBy.toLocaleString()}</td>
                        <td>{d.totalVP}</td>
                        <td className="gh__delarrow"><Chevron /></td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
              <a className="gh__fullbtn" href="/governance/delegate">View all Delegates</a>
            </section>

            <section className="gh__section gh__section--engagement" id="engagement">
              <div className="gh__enghead">
                <h2 className="gh__engtitle">Community Engagement</h2>
                <p className="gh__engdesc">
                  See the participation signal from the community and understand the basic health stats from the DAO.
                </p>
              </div>
              <div className="gh__engdata">
                {chartPoints.length > 1 && (
                  <div className="gh__chartcard">
                    <BoxTabs
                      tabs={CHART_TABS}
                      active={chartTab}
                      onSelect={setChartTab}
                      right={<span className="gh__chartdisplay">Display: median</span>}
                    />
                    <LineChart points={chartPoints} />
                  </div>
                )}
                <div className="gh__voterscard">
                  <table className="gh__voterstable">
                    <thead>
                      <tr>
                        <th>Last 30-Days top voters</th>
                        <th className="gh__voterscount">Votes</th>
                      </tr>
                    </thead>
                    <tbody>
                      {engagementUnavailable && (
                        <tr>
                          <td colSpan={2} className="gh__votersunavailable">
                            Engagement data could not be read from this node.
                          </td>
                        </tr>
                      )}
                      {topVoters.map((v) => (
                        <tr key={v.rank}>
                          <td>
                            <span className="gh__voteruser">
                              <span className="gh__voterrank">{v.rank}</span>
                              <span className="gh__voteravatar u-avatar" style={{ "--sz": "24px", "--hue": v.hue } as CSSProperties & { "--sz": string; "--hue": number }} aria-hidden="true" />
                              <span className="gh__votername">{v.name}</span>
                            </span>
                          </td>
                          <td className="gh__voterscount">{v.votes}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              </div>
            </section>

            <div className="gh__bottom">
              <div className="gh__bottominfo">
                <h2 className="gh__bottomtitle">
                  We have so much ahead of us. The future of the Metaverse is in your hands!
                </h2>
                {bottomStats.length > 0 && (
                  <div className="gh__bottomstats">
                    {bottomStats.map((s) => (
                      <div className="gh__stat" key={s.label}>
                        <div className="gh__statval">{s.value}</div>
                        <div className="gh__statdesc">{s.label}</div>
                      </div>
                    ))}
                  </div>
                )}
              </div>
              <div className="gh__bottomactions">
                {[
                  { title: "Join our Discord Channel", desc: "Engage with the community", href: "https://dcl.gg/discord" },
                  { title: "Debate on the forum", desc: "Discuss proposals and more", href: "https://forum.decentraland.org/" },
                ].map((a) => (
                  <a className="gh__action" key={a.title} href={a.href} target="_blank" rel="noopener noreferrer">
                    <span className="gh__actionicon" aria-hidden="true">
                      <CommentBubble />
                    </span>
                    <span className="gh__actioninfo">
                      <span className="gh__actiontitle">{a.title}</span>
                      <span className="gh__actiondesc">{a.desc}</span>
                    </span>
                  </a>
                ))}
              </div>
            </div>
          </div>

          <aside className="gh__ticker" aria-label="Latest activity">
            <div className="gh__tickerhead">
              <span className="gh__tickergradient" aria-hidden="true" />
              <h2 className="gh__tickertitle">Latest activity</h2>
              <button
                type="button"
                className="gh__tickerfilter"
                aria-expanded={tickerFilterOpen}
                onClick={() => setTickerFilterOpen((o) => !o)}
              >
                <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" aria-hidden="true">
                  <path d="M3 5h18M6 12h12M10 19h4" />
                </svg>
                {tickerKind === "all" ? "Filter" : TICKER_KIND_LABELS[tickerKind] ?? tickerKind}
              </button>
              {tickerFilterOpen && (
                <div className="gh__tickermenu" role="group" aria-label="Filter activity">
                  {["all", ...tickerKinds].map((k) => (
                    <button
                      key={k}
                      type="button"
                      className={"gh__tickeropt" + (tickerKind === k ? " is-active" : "")}
                      aria-pressed={tickerKind === k}
                      onClick={() => {
                        setTickerKind(k);
                        setTickerFilterOpen(false);
                      }}
                    >
                      {k === "all" ? "All activity" : TICKER_KIND_LABELS[k] ?? k}
                    </button>
                  ))}
                </div>
              )}
            </div>
            <div className="gh__tickerlist" role="region" aria-label="Activity feed" tabIndex={0}>
              {tickerItems.map((item) => (
                <ActivityItem key={item.id} item={item} />
              ))}
              {tickerItems.length === 0 && (
                <div className="gh__tickerempty">
                  {activityUnavailable
                    ? "Latest activity could not be read from this node."
                    : "No matching activity"}
                </div>
              )}
            </div>
          </aside>
        </div>
      </div>
    </GovernanceChrome>
  );
}
