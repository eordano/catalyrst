import type { CSSProperties, ReactNode } from "react";
import { useMemo, useState } from "react";
import GovernanceChrome, { type GovernanceNavId } from "../frames/GovernanceChrome";
import "./gvprofileactivity.css";
import { Caret } from "../../atoms/icons";

type LabelInfo = { label: string; tone: string };

type Proposal = {
  id: string;
  title: string;
  status: string;
  category: string;
  votes?: number | null;
  date: string;
  choice?: string;
  stance?: string;
  pending?: boolean | null;
};

type Project = {
  id: string;
  title: string;
  role: string;
  amount: number;
  token: string;
  passed: string;
  funded: number;
};

type Delegator = { address: string; vp: number; hue: number };

function fmtNum(n: number) {
  return n.toLocaleString("en-US");
}

export type GvProfileProposal = Proposal;
export type GvProfileProject = Project;
export type GvProfileDelegator = Delegator;
export type GvProfileVp = { total: number; own: number; delegated: number };
export type GvProfileVpSegment = { id: string; label: string; value: number; tone: string };
export type GvProfileVotingStats = {
  participationTotal: number;
  participationPercentage: string;
  personalMatchPercentage: number;
  outcomeMatchPercentage: number;
};
export type GvProfileBadge = { id: string | number; label: string; hue: number };

const CATEGORY_LABELS: Record<string, LabelInfo> = {
  catalyst: { label: "Catalyst Node", tone: "blue" },
  grant: { label: "Grant Request", tone: "purple" },
  poll: { label: "Poll", tone: "orange" },
  tender: { label: "Tender", tone: "red" },
  governance: { label: "Governance", tone: "orange" },
  linked_wearables: { label: "Linked Wearables", tone: "yellow" },
  poi: { label: "Point of Interest", tone: "green" },
  ban_name: { label: "Name Ban", tone: "fuchsia" },
};
const STATUS_LABELS: Record<string, LabelInfo> = {
  active: { label: "Active", tone: "neutral" },
  passed: { label: "Passed", tone: "green" },
  enacted: { label: "Enacted", tone: "green" },
  rejected: { label: "Rejected", tone: "red" },
  finished: { label: "Finished", tone: "neutral" },
  out_of_budget: { label: "Out of Budget", tone: "yellow" },
};

const HelperIcon = () => (
  <svg viewBox="0 0 16 16" width="13" height="13" aria-hidden="true" className="gpa__helper">
    <circle cx="8" cy="8" r="7" fill="none" stroke="currentColor" strokeWidth="1.4" />
    <path d="M8 11.5v.01M6.4 6.1a1.6 1.6 0 1 1 2.4 1.4c-.5.3-.8.6-.8 1.2" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
  </svg>
);
const ValidatedCheck = () => (
  <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true" className="gpa__validated">
    <path d="M12 2 4 5v6c0 5 3.4 8.4 8 11 4.6-2.6 8-6 8-11V5l-8-3Z" fill="var(--brand)" />
    <path d="M8.5 12l2.3 2.3L15.5 9.5" fill="none" stroke="#fff" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
  </svg>
);
const BoltIcon = () => (
  <svg viewBox="0 0 24 24" width="11" height="11" aria-hidden="true">
    <path d="M13 2 4 14h6l-1 8 9-12h-6l1-8Z" fill="currentColor" />
  </svg>
);
const Chevron = () => (
  <svg viewBox="0 0 24 24" width="22" height="22" aria-hidden="true" className="gpa__chev">
    <circle cx="12" cy="12" r="10.25" fill="none" stroke="currentColor" strokeWidth="1.5" />
    <path d="M10.5 8.5l3.5 3.5-3.5 3.5" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" />
  </svg>
);
const StanceIcon = ({ stance }: { stance?: string }) => {
  if (stance === "shared")
    return (
      <svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">
        <circle cx="8" cy="8" r="7" fill="none" stroke="currentColor" strokeWidth="1.4" />
        <path d="M5 8.2l2 2 4-4.4" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
      </svg>
    );
  if (stance === "differs")
    return (
      <svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">
        <circle cx="8" cy="8" r="7" fill="none" stroke="currentColor" strokeWidth="1.4" />
        <path d="M5.5 5.5l5 5M10.5 5.5l-5 5" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
      </svg>
    );
  return (
    <svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">
      <circle cx="8" cy="8" r="7" fill="none" stroke="currentColor" strokeWidth="1.4" />
      <path d="M8 4.5V8l2.4 1.4" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
};

type ActionBoxProps = {
  title: ReactNode;
  info?: string;
  action?: ReactNode;
  className?: string;
  children?: ReactNode;
};

function ActionBox({ title, info, action, className = "", children }: ActionBoxProps) {
  return (
    <div className={"gpa__box " + className}>
      <div className="gpa__boxhead">
        <div className="gpa__boxtitle">
          <span>{title}</span>
          {info && (
            <span className="gpa__helperbtn" title={info}>
              <HelperIcon />
            </span>
          )}
        </div>
        {action && <div className="gpa__boxaction">{action}</div>}
      </div>
      <div className="gpa__divider" />
      <div className="gpa__boxbody">{children}</div>
    </div>
  );
}

type VpStatBoxProps = { title: ReactNode; value: number; info?: string };

function VpStatBox({ title, value, info }: VpStatBoxProps) {
  return (
    <div className="gpa__statbox">
      <div className="gpa__stathead">
        <span className="gpa__stattitle">{title}</span>
        {info && (
          <span className="gpa__helperbtn" title={info}>
            <HelperIcon />
          </span>
        )}
      </div>
      <div className="gpa__vpvalue">
        <span className="gpa__vpmark">
          <BoltIcon />
        </span>
        {fmtNum(value)}
      </div>
    </div>
  );
}

type VotingStatBoxProps = {
  title: ReactNode;
  main: ReactNode;
  sub: ReactNode;
  info?: string;
};

function VotingStatBox({ title, main, sub, info }: VotingStatBoxProps) {
  return (
    <div className="gpa__statbox">
      <div className="gpa__stathead">
        <span className="gpa__stattitle">{title}</span>
        {info && (
          <span className="gpa__helperbtn" title={info}>
            <HelperIcon />
          </span>
        )}
      </div>
      <div className="gpa__votingdata">
        <span className="gpa__votingmain">{main}</span>
        <span className="gpa__votingsub">{sub}</span>
      </div>
    </div>
  );
}

function ProposalRow({ p, kind }: { p: Proposal; kind: string }) {
  const cat = CATEGORY_LABELS[p.category];
  const status = STATUS_LABELS[p.status];
  return (
    <a className="gpa__prow" href={`/governance/proposals/${p.id}`}>
      <div className="gpa__prowmain">
        <h4 className="gpa__prowtitle">{p.title}</h4>
        <div className="gpa__prowstatus">
          {status && <span className={"gpa__status gpa__status--" + status.tone}>{status.label}</span>}
          {cat && <span className={"gpa__pill gpa__pill--" + cat.tone}>{cat.label}</span>}
          {p.pending && <span className="gpa__pill gpa__pill--red">Pending request</span>}
          {kind === "voted" && (
            <span className="gpa__prowdetail">They voted &ldquo;{p.choice}&rdquo;</span>
          )}
          {kind === "created" && p.votes != null && <span className="gpa__prowdetail">{p.votes} votes</span>}
          <span className="gpa__prowdetail">{p.date}</span>
        </div>
      </div>
      <div className="gpa__prowend">
        {kind === "voted" && (
          <span className={"gpa__stance gpa__stance--" + p.stance}>
            <StanceIcon stance={p.stance} />
            {p.stance === "shared" ? "Stance shared" : p.stance === "differs" ? "Stance differs" : "Voting in progress"}
          </span>
        )}
        {kind === "created" && p.status === "active" && <span className="gpa__votetext">Vote</span>}
        <Chevron />
      </div>
    </a>
  );
}

function FullWidthButton({ children }: { children?: ReactNode }) {
  return (
    <button type="button" className="gpa__fullbtn">
      {children}
    </button>
  );
}

function DelegatorCard({ d }: { d: Delegator }) {
  return (
    <span className="gpa__delcard" role="button" tabIndex={0} onClick={(e) => e.preventDefault()}>
      <div className="gpa__delsection">
        <span className="gpa__delavatar u-avatar" style={{ "--sz": "40px", "--hue": d.hue } as CSSProperties} aria-hidden="true" />
        <div className="gpa__deldetails">
          <span className="gpa__deltitle">{d.address}</span>
          <span className="gpa__delvp">Delegated {fmtNum(d.vp)} VP</span>
        </div>
      </div>
      <Chevron />
    </span>
  );
}

function ProjectItem({ p }: { p: Project }) {
  return (
    <a className="gpa__project" href={`/governance/projects/${p.id}`}>
      <div className="gpa__projmain">
        <h4 className="gpa__projtitle">{p.title}</h4>
        <p className="gpa__projdesc">
          Passed {p.passed} with a budget of{" "}
          <b>
            {fmtNum(p.amount)} {p.token}
          </b>
        </p>
      </div>
      <div className="gpa__projfund">
        <div className="gpa__fundbar" role="img" aria-label={`${Math.round(p.funded * 100)}% funded`}>
          <span className="gpa__fundfill" style={{ width: Math.round(p.funded * 100) + "%" }} />
        </div>
        <span className="gpa__fundpct">{Math.round(p.funded * 100)}%</span>
      </div>
      <Chevron />
    </a>
  );
}

type GvProfileActivityProps = {
  username?: string;
  address?: string;
  bio?: string;
  isOwnProfile?: boolean;
  validated?: boolean;
  vp?: GvProfileVp;
  vpDistribution?: GvProfileVpSegment[];
  votingStats?: GvProfileVotingStats;
  badges?: GvProfileBadge[];
  badgesMore?: number;
  projects?: Project[];
  proposals?: Proposal[];
  watchlist?: Proposal[];
  coauthoring?: Proposal[];
  delegatedTo?: Delegator | null;
  delegators?: Delegator[];
  voted?: Proposal[];
};

export default function GvProfileActivity({
  username = "",
  address = "",
  bio = "",
  isOwnProfile = false,
  validated = false,
  vp,
  vpDistribution = [],
  votingStats,
  badges = [],
  badgesMore = 0,
  projects = [],
  proposals = [],
  watchlist = [],
  coauthoring = [],
  delegatedTo = null,
  delegators = [],
  voted = [],
}: GvProfileActivityProps) {
  const [tab, setTab] = useState<GovernanceNavId | null>(null);
  const [activity, setActivity] = useState("myProposals");

  const activityList = useMemo(() => {
    if (activity === "watchlist") return watchlist;
    if (activity === "coauthoring") return coauthoring;
    return proposals;
  }, [activity, watchlist, coauthoring, proposals]);

  const segTotal = vpDistribution.reduce((s, x) => s + x.value, 0);
  const hasPendingCoauthor = coauthoring.some((c) => c.pending);

  return (
    <GovernanceChrome active={tab} onTab={setTab} account={address}>
      <div className="gpa">
        <section className="gpa__userstats">
          <div>
            <div className="gpa__usernamerow">
              <div className="gpa__usernamebox">
                <span className="gpa__username">{username || address}</span>
                {validated && <ValidatedCheck />}
                {isOwnProfile && (
                  <button type="button" className="gpa__settings" title="Profile settings" aria-label="Profile settings">
                    <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" strokeWidth="1.6">
                      <circle cx="12" cy="12" r="3" />
                      <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-2.81 1.17V21a2 2 0 1 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.6 15H4.5a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 11 4.6V4.5a2 2 0 1 1 4 0v.09A1.65 1.65 0 0 0 18 6a1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 11h.1a2 2 0 1 1 0 4h-.1Z" />
                    </svg>
                  </button>
                )}
              </div>
              {isOwnProfile && (
                <button type="button" className="gpa__getvp">
                  <BoltIcon />
                  Get More VP
                  <Caret size={12} />
                </button>
              )}
            </div>

            {badges.length > 0 && (
              <div className="gpa__badges" aria-label="Badges">
                {badges.map((b) => (
                  <span key={b.id} className="gpa__badge u-avatar" style={{ "--sz": "44px", "--hue": b.hue } as CSSProperties} title={b.label} aria-label={b.label} />
                ))}
                {badgesMore > 0 && <span className="gpa__badgemore">+{badgesMore} More</span>}
              </div>
            )}
          </div>

          <div className="gpa__userinfo">
            <div className="gpa__userinfomain">
              {bio && (
                <div className="gpa__bio">
                  <span className="gpa__biolabel">Bio</span>
                  <p className="gpa__biotext">{bio}</p>
                </div>
              )}

              {vp && (
                <div className="gpa__statrow">
                  <VpStatBox title="Consolidated Voting Power" value={vp.total} info="Results from the sum of own Voting Power and delegated Voting Power" />
                  <VpStatBox title="Own Voting Power" value={vp.own} info="Voting Power that results from holding/owning MANA, L1 Wearables, Names, LAND and/or ESTATE" />
                  <VpStatBox title="Delegated Voting Power" value={vp.delegated} info="Voting Power that results from the delegation of VP by other users onto this account" />
                </div>
              )}

              {vpDistribution.length > 0 && segTotal > 0 && (
                <ActionBox title="Voting Power Distribution" className="gpa__vpdistbox">
                  <div className="gpa__vpdist">
                    <div className="gpa__vpdistbar" aria-hidden="true">
                      {vpDistribution.map((s) => (
                        <span
                          key={s.id}
                          className={"gpa__vpseg gpa__seg--" + s.tone}
                          style={{ width: (s.value / segTotal) * 100 + "%" }}
                        />
                      ))}
                    </div>
                    <ul className="gpa__vplegend">
                      {vpDistribution.map((s) => (
                        <li key={s.id} className="gpa__vplegitem">
                          <span className={"gpa__legdot gpa__seg--" + s.tone} aria-hidden="true" />
                          <span className="gpa__leglabel">{s.label}</span>
                          <span className="gpa__legval">{fmtNum(s.value)}</span>
                        </li>
                      ))}
                    </ul>
                  </div>
                </ActionBox>
              )}

              {votingStats && (
                <div className="gpa__statrow">
                  <VotingStatBox
                    title="30-Day Participation"
                    main={`Voted on ${votingStats.participationTotal} proposals`}
                    sub={`${votingStats.participationPercentage} participation rate`}
                  />
                  <VotingStatBox
                    title="Personal Match"
                    info="Personal match shows the alignment you have with this profile in terms of voting decisions."
                    main={`You're ${votingStats.personalMatchPercentage}% aligned`}
                    sub="Conversations to be had"
                  />
                  <VotingStatBox
                    title="Outcome Match"
                    info='When a vote coincides with the outcome, it&apos;s considered to be a "match".'
                    main={`${votingStats.outcomeMatchPercentage}% match`}
                    sub="Aligned with the community"
                  />
                </div>
              )}
            </div>

            <div className="gpa__avatar" aria-hidden="true">
              <div className="gpa__avatarrender u-avatar" style={{ "--sz": "100%", "--hue": 268 } as CSSProperties} />
            </div>
          </div>
        </section>

        {projects.length > 0 && (
          <ActionBox
            title="Projects"
            info="Projects that this account has authored or co-authored and have been approved and funded by the Decentraland DAO."
          >
            <div className="gpa__projects">
              {projects.map((p) => (
                <ProjectItem key={p.id} p={p} />
              ))}
            </div>
          </ActionBox>
        )}

        <div className="gpa__tabbox">
          <div className="gpa__boxtabs" role="tablist">
            <button
              type="button"
              role="tab"
              aria-selected={activity === "myProposals"}
              className={"gpa__tab" + (activity === "myProposals" ? " is-active" : "")}
              onClick={() => setActivity("myProposals")}
            >
              My proposals
            </button>
            {isOwnProfile && (
              <button
                type="button"
                role="tab"
                aria-selected={activity === "watchlist"}
                className={"gpa__tab" + (activity === "watchlist" ? " is-active" : "")}
                onClick={() => setActivity("watchlist")}
              >
                Watchlist
              </button>
            )}
            <button
              type="button"
              role="tab"
              aria-selected={activity === "coauthoring"}
              className={"gpa__tab" + (activity === "coauthoring" ? " is-active" : "")}
              onClick={() => setActivity("coauthoring")}
            >
              Co-authoring
              {hasPendingCoauthor && <span className="gpa__dot" aria-label="Pending requests" />}
            </button>
          </div>
          <div className="gpa__tabcontent">
            {activityList.length > 0 ? (
              <>
                <div className="gpa__prows">
                  {activityList.map((p) => (
                    <ProposalRow key={p.id} p={p} kind="created" />
                  ))}
                </div>
                <FullWidthButton>View more proposals</FullWidthButton>
              </>
            ) : (
              <div className="gpa__empty gpa__empty--tab">
                <span className="gpa__emptymelon" aria-hidden="true">🍉</span>
                <p className="gpa__emptytext">
                  {activity === "watchlist"
                    ? "You haven't subscribed to any proposal yet"
                    : activity === "coauthoring"
                    ? "You haven't co-authored any proposal yet"
                    : "You haven't submitted any proposal yet"}
                </p>
              </div>
            )}
          </div>
        </div>

        {delegatedTo && (
          <ActionBox
            title="Delegated VP to"
            info="This is the DAO member this user delegated their VP to."
            action={
              isOwnProfile && (
                <button type="button" className="gpa__basicbtn">
                  Change Delegation
                </button>
              )
            }
          >
            <div className="gpa__delgrid">
              <DelegatorCard d={delegatedTo} />
            </div>
          </ActionBox>
        )}

        {delegators.length > 0 && (
          <ActionBox
            title="VP Delegators"
            info="These are all the DAO members that delegated their VP to this user."
          >
            <div className="gpa__delgrid">
              {delegators.map((d) => (
                <DelegatorCard key={d.address} d={d} />
              ))}
            </div>
          </ActionBox>
        )}

        {voted.length > 0 && (
          <ActionBox title="Proposals voted on">
            <div className="gpa__prows">
              {voted.map((p) => (
                <ProposalRow key={p.id} p={p} kind="voted" />
              ))}
            </div>
            <FullWidthButton>View more proposals</FullWidthButton>
          </ActionBox>
        )}
      </div>
    </GovernanceChrome>
  );
}
