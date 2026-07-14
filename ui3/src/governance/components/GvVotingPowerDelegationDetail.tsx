import type { CSSProperties, ReactNode } from "react";
import { useMemo, useState } from "react";
import Modal from "../../components/Modal";
import { ChevronLeft, Close } from "../../atoms/icons";
import "./gvvotingpowerdelegationdetail.css";

const COPY = {
  backLabel: "Go back to delegates list",
  delegateButton: "Delegate VP",
  about: "About",
  links: "Links",
  relevantSkills: "Relevant Skills",
  involvement: "Involvement with Decentraland",
  motivation: "Motivation to be a delegate",
  vision: "5-year vision for Decentraland",
  mostImportantIssue: "Most important issue in Decentraland",
  showMore: "Show more",
  showLess: "Show less",
  statsLoading: "Compiling profile...",
  ownVP: "Own voting power",
  delegatedVP: "Delegated voting power",
  totalVP: "Total voting power",
  mana: "MANA",
  land: "LAND",
  names: "NAMES",
  estate: "ESTATE",
  l1Wearables: "L1 WEARABLES",
  activeSince: "Active since",
  votedOn: "Voted on",
  match: "Match",
  initiativesTitle: "Initiatives voted on",
  voted: "Voted ",
  viewMore: "View more",
};

type DistributionKey =
  | "mana"
  | "names"
  | "l1Wearables"
  | "land"
  | "estate"
  | "delegated"
  | "rental";

const DISTRIBUTION: { key: DistributionKey; label: string; tone: string }[] = [
  { key: "mana", label: "MANA", tone: "mana" },
  { key: "names", label: "NAMES", tone: "name" },
  { key: "l1Wearables", label: "L1 Wearables", tone: "l1" },
  { key: "land", label: "LAND", tone: "land" },
  { key: "estate", label: "Estates", tone: "estate" },
  { key: "delegated", label: "Delegated", tone: "delegated" },
  { key: "rental", label: "LAND Rental", tone: "rental" },
];

export type GvDelegationCandidate = {
  address: string;
  name: string;
  hue: number;
  bio: string;
  involvement: string;
  motivation: string;
  vision: string;
  most_important_issue: string;
  links: string[];
  relevant_skills: string[];
};

type Candidate = GvDelegationCandidate;

export type GvVpDistribution = {
  total: number;
  own: number;
  delegated: number;
  mana: number;
  names: number;
  l1Wearables: number;
  land: number;
  estate: number;
  rental: number;
};

type VpDistribution = GvVpDistribution;

const EMPTY_VP_DISTRIBUTION: VpDistribution = {
  total: 0,
  own: 0,
  delegated: 0,
  mana: 0,
  names: 0,
  l1Wearables: 0,
  land: 0,
  estate: 0,
  rental: 0,
};

export type GvDelegationVote = {
  id: string;
  title: string;
  choice: string;
  match?: boolean;
};

type Vote = GvDelegationVote;

const nf = new Intl.NumberFormat("en-US");
function pct(value: number, total: number): string {
  if (!total) return "0%";
  return Math.round((value / total) * 100) + "%";
}

const LinkIcon = () => (
  <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d="M10 13a5 5 0 0 0 7 0l3-3a5 5 0 0 0-7-7l-1 1" />
    <path d="M14 11a5 5 0 0 0-7 0l-3 3a5 5 0 0 0 7 7l1-1" />
  </svg>
);
const CheckCircle = () => (
  <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="var(--gvd-green)" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <circle cx="12" cy="12" r="10" />
    <path d="M8 12l3 3 5-6" />
  </svg>
);
const CancelCircle = () => (
  <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="var(--gvd-red)" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <circle cx="12" cy="12" r="10" />
    <path d="M15 9l-6 6M9 9l6 6" />
  </svg>
);
const QuestionCircle = () => (
  <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="var(--gvd-faint)" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <circle cx="12" cy="12" r="10" />
    <path d="M9.5 9a2.5 2.5 0 0 1 4.5 1.5c0 1.5-2 2-2 3.5" />
    <path d="M12 17h.01" />
  </svg>
);
const Bolt = () => (
  <svg viewBox="0 0 24 24" width="0.78em" height="0.78em" aria-hidden="true" className="gvvotingpowerdelegationdetail__bolt">
    <path d="M13 2 4 14h6l-1 8 9-12h-6l1-8Z" fill="currentColor" />
  </svg>
);

type VotingPowerProps = {
  value: number;
  size?: "medium" | "large";
};

function VotingPower({ value, size = "medium" }: VotingPowerProps) {
  return (
    <div className={"gvvotingpowerdelegationdetail__vp gvvotingpowerdelegationdetail__vp--" + size}>
      <span>{nf.format(value)}</span>
      <Bolt />
    </div>
  );
}

type StatProps = {
  title: string;
  children?: ReactNode;
};

function Stat({ title, children }: StatProps) {
  return (
    <div className="gvvotingpowerdelegationdetail__stat">
      <span className="gvvotingpowerdelegationdetail__statlabel">{title}</span>
      {children}
    </div>
  );
}

type CandidateDetailsProps = {
  title: string;
  content?: string;
  links?: string[];
  skills?: string[];
};

function CandidateDetails({ title, content, links, skills }: CandidateDetailsProps) {
  if ((links && links.length === 0) || (skills && skills.length === 0)) return null;
  if (!content && !links?.length && !skills?.length) return null;
  return (
    <div className="gvvotingpowerdelegationdetail__detail">
      <h4 className="gvvotingpowerdelegationdetail__detailtitle">{title}</h4>
      {content ? <p className="gvvotingpowerdelegationdetail__detailtext">{content}</p> : null}
      {links ? (
        <div className="gvvotingpowerdelegationdetail__links">
          {links.map((href, i) => (
            <a key={i} className="gvvotingpowerdelegationdetail__link" href={href}>
              <LinkIcon /> {href.replace(/^https?:\/\//i, "").replace(/^www\./i, "")}
            </a>
          ))}
        </div>
      ) : null}
      {skills ? (
        <div className="gvvotingpowerdelegationdetail__chips">
          {skills.map((s, i) => (
            <span key={i} className="gvvotingpowerdelegationdetail__chip">{s.toUpperCase()}</span>
          ))}
        </div>
      ) : null}
    </div>
  );
}

type VotedInitiativeProps = {
  vote: Vote;
};

function VotedInitiative({ vote }: VotedInitiativeProps) {
  const icon =
    vote.match === undefined ? <QuestionCircle /> : vote.match ? <CheckCircle /> : <CancelCircle />;
  return (
    <a className="gvvotingpowerdelegationdetail__vote" href={`/governance/proposals/${vote.id}`}>
      <div className="gvvotingpowerdelegationdetail__votetitlewrap">
        {icon}
        <h2 className="gvvotingpowerdelegationdetail__votetitle">{vote.title}</h2>
      </div>
      <div className="gvvotingpowerdelegationdetail__votechoice">
        {COPY.voted}
        <span className="gvvotingpowerdelegationdetail__votechoicehl">{vote.choice}</span>
      </div>
    </a>
  );
}

const VOTES_PER_PAGE = 4;

type GvVotingPowerDelegationDetailProps = {
  candidate?: Candidate;
  vpDistribution?: VpDistribution;
  votes?: Vote[];
  matchPercentage?: number;
  activeSince?: string;
  userVP?: string;
  loading?: boolean;
  onBack?: () => void;
  onClose?: () => void;
};

export default function GvVotingPowerDelegationDetail({
  candidate,
  vpDistribution = EMPTY_VP_DISTRIBUTION,
  votes = [],
  matchPercentage = 0,
  activeSince = "",
  userVP = "",
  loading = false,
  onBack,
  onClose,
}: GvVotingPowerDelegationDetailProps) {
  const [expanded, setExpanded] = useState(false);
  const [shown, setShown] = useState(VOTES_PER_PAGE);

  const visibleVotes = useMemo(() => votes.slice(0, shown), [votes, shown]);
  const hasShownAll = shown >= votes.length;

  const matchColor = `rgb(0, ${Math.round((200 * matchPercentage) / 100)}, 0)`;

  return (
    <Modal width="100%" className="modal__card--plain gvvotingpowerdelegationdetail__card gv" ariaLabel="Delegate detail">
        <button
          type="button"
          className="gvvotingpowerdelegationdetail__close"
          aria-label="Close"
          onClick={onClose}
        >
          <Close size={16} />
        </button>

        <header className="gvvotingpowerdelegationdetail__header">
          <div className="gvvotingpowerdelegationdetail__candidate">
            <button
              type="button"
              className="gvvotingpowerdelegationdetail__back"
              aria-label={COPY.backLabel}
              onClick={onBack}
            >
              <ChevronLeft size={18} strokeWidth={2.2} />
            </button>
            <span className="u-avatar gvvotingpowerdelegationdetail__avatar" style={{ "--sz": "32px", "--hue": candidate?.hue ?? 0 } as CSSProperties & { "--sz": string; "--hue": number }} aria-hidden="true" />
            <span className="gvvotingpowerdelegationdetail__name">{candidate?.name}</span>
          </div>
          <div className="gvvotingpowerdelegationdetail__delegate">
            {userVP ? <span className="gvvotingpowerdelegationdetail__delegatehint">Delegated {userVP} VP</span> : null}
            <button type="button" className="gvvotingpowerdelegationdetail__delegatebtn">{COPY.delegateButton}</button>
          </div>
        </header>

        <div className={"gvvotingpowerdelegationdetail__info" + (expanded ? " is-expanded" : "")}>
          <div className="gvvotingpowerdelegationdetail__detailgrid">
            <div>
              <CandidateDetails title={COPY.about} content={candidate?.bio} />
              <CandidateDetails title={COPY.involvement} content={candidate?.involvement} />
              <CandidateDetails title={COPY.motivation} content={candidate?.motivation} />
              <CandidateDetails title={COPY.vision} content={candidate?.vision} />
              <CandidateDetails title={COPY.mostImportantIssue} content={candidate?.most_important_issue} />
            </div>
            <div>
              <CandidateDetails title={COPY.links} links={candidate?.links} />
              <CandidateDetails title={COPY.relevantSkills} skills={candidate?.relevant_skills} />
            </div>
          </div>
          {!expanded ? <div className="gvvotingpowerdelegationdetail__fadeout" /> : null}
        </div>

        <div className="gvvotingpowerdelegationdetail__showmore">
          <div className="gvvotingpowerdelegationdetail__divider" />
          <button
            type="button"
            className="gvvotingpowerdelegationdetail__showmorebtn"
            onClick={() => setExpanded((p) => !p)}
          >
            {expanded ? COPY.showLess : COPY.showMore}
          </button>
        </div>

        {loading ? (
          <div className="gvvotingpowerdelegationdetail__loading">
            <span className="gvvotingpowerdelegationdetail__spinner" aria-hidden="true" />
            <span className="gvvotingpowerdelegationdetail__loadingtext">{COPY.statsLoading}</span>
          </div>
        ) : (
          <>
            <div className="gvvotingpowerdelegationdetail__stats">
              <Stat title={COPY.ownVP}><VotingPower value={vpDistribution.own} size="large" /></Stat>
              <Stat title={COPY.delegatedVP}><VotingPower value={vpDistribution.delegated} size="large" /></Stat>
              <Stat title={COPY.totalVP}><VotingPower value={vpDistribution.total} size="large" /></Stat>

              <Stat title={COPY.mana}><VotingPower value={vpDistribution.mana} /></Stat>
              <Stat title={COPY.names}><VotingPower value={vpDistribution.names} /></Stat>
              <Stat title={COPY.l1Wearables}><VotingPower value={vpDistribution.l1Wearables} /></Stat>
              <Stat title={COPY.land}><VotingPower value={vpDistribution.land} /></Stat>
              <Stat title={COPY.estate}><VotingPower value={vpDistribution.estate} /></Stat>

              <div className="gvvotingpowerdelegationdetail__distribution">
                <div className="gvvotingpowerdelegationdetail__bar">
                  {DISTRIBUTION.map(({ key, tone }) =>
                    vpDistribution[key] > 0 ? (
                      <div
                        key={key}
                        className={"gvvotingpowerdelegationdetail__barseg gvvotingpowerdelegationdetail__barseg--" + tone}
                        style={{ width: pct(vpDistribution[key], vpDistribution.total) }}
                        title={`${nf.format(vpDistribution[key])} VP (${pct(vpDistribution[key], vpDistribution.total)})`}
                      />
                    ) : null
                  )}
                </div>
                <div className="gvvotingpowerdelegationdetail__barlabels">
                  {DISTRIBUTION.map(({ key, label, tone }) =>
                    vpDistribution[key] > 0 ? (
                      <span key={key} className="gvvotingpowerdelegationdetail__barlabel">
                        <span className={"gvvotingpowerdelegationdetail__bardot gvvotingpowerdelegationdetail__barseg--" + tone} aria-hidden="true" />
                        {label}
                      </span>
                    ) : null
                  )}
                </div>
              </div>

              {activeSince ? (
                <Stat title={COPY.activeSince}>
                  <div className="gvvotingpowerdelegationdetail__statvalue">{activeSince}</div>
                </Stat>
              ) : null}
              <Stat title={COPY.votedOn}>
                <div className="gvvotingpowerdelegationdetail__statvalue">{votes.length}</div>
              </Stat>
              {matchPercentage > 0 ? (
                <Stat title={COPY.match}>
                  <div className="gvvotingpowerdelegationdetail__statvalue" style={{ color: matchColor }}>
                    {matchPercentage}%
                  </div>
                </Stat>
              ) : null}
            </div>

            {visibleVotes.length > 0 ? (
              <div className="gvvotingpowerdelegationdetail__votes">
                <span className="gvvotingpowerdelegationdetail__voteslabel">{COPY.initiativesTitle}</span>
                <div className="gvvotingpowerdelegationdetail__voteslist">
                  {visibleVotes.map((v) => (
                    <VotedInitiative key={v.id} vote={v} />
                  ))}
                </div>
              </div>
            ) : null}
          </>
        )}

        {!loading && !hasShownAll ? (
          <button
            type="button"
            className="gvvotingpowerdelegationdetail__viewmore"
            onClick={() => setShown((s) => s + VOTES_PER_PAGE)}
          >
            {COPY.viewMore}
          </button>
        ) : null}
    </Modal>
  );
}
