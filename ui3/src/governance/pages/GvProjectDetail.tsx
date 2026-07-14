import { useState } from "react";
import { GovernanceChromeMaybe, type GovernanceNavId } from "../frames/GovernanceChrome";
import "./gvprojectdetail.css";

type ProjectLink = { id: string; label: string; url: string };
type Personnel = { id: string; name: string; address: string | null; role: string; about: string };
type Milestone = { id: string; date: string; title: string; description: string };
type Funding = {
  enactedLabel: string;
  endLabel: string;
  total: string;
  token: string;
  vestedAmount: string;
  vestedPct: number;
  releasedAmount: string;
  releasedPct: number;
  nextVested: { time: number; unit: string; amount: string };
};
type Vesting = { id: string; label: string; url: string };
export type GvProject = {
  id: string;
  proposal_id: string;
  title: string;
  status: string;
  ongoingDays: number;
  about: string;
  links: ProjectLink[];
  personnel: Personnel[];
  milestones: Milestone[];
  funding: Funding;
  vestings: Vesting[];
};
type Project = GvProject;

const STATUS_PILL: Record<string, string | ((p: Project) => string)> = {
  pending: "In refinement",
  in_progress: (p) => `Ongoing for ${p.ongoingDays} days`,
  finished: "Ended",
  paused: "Paused",
  revoked: "Revoked",
};

type StatusCard = { title: string; icon: string };
const STATUS_CARD: Record<string, StatusCard> = {
  in_progress: { title: "Project Ongoing", icon: "in_progress" },
  finished: { title: "Project Ended Successfully", icon: "finished" },
  paused: { title: "Project Paused Preventively", icon: "paused" },
  revoked: { title: "Project Revoked", icon: "revoked" },
};

const MENU = [
  { id: "general_info", label: "General Info" },
  { id: "milestones", label: "Milestones" },
  { id: "updates", label: "Updates" },
  { id: "activity", label: "Activity" },
];

const Dot = () => <span className="gpd__dot" aria-hidden="true" />;

const StatusIcon = ({ kind }: { kind: string }) => {
  const common = { width: 24, height: 24, viewBox: "0 0 24 24", fill: "none", "aria-hidden": true };
  if (kind === "finished")
    return (
      <svg {...common}>
        <circle cx="12" cy="12" r="10" fill="var(--tone-green-t)" />
        <path d="M7.5 12.2l3 3 6-6.4" stroke="#fff" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
      </svg>
    );
  if (kind === "revoked")
    return (
      <svg {...common}>
        <circle cx="12" cy="12" r="10" fill="var(--tone-red-t)" />
        <path d="M8.5 8.5l7 7M15.5 8.5l-7 7" stroke="#fff" strokeWidth="1.8" strokeLinecap="round" />
      </svg>
    );
  if (kind === "paused")
    return (
      <svg {...common}>
        <circle cx="12" cy="12" r="10" fill="var(--tone-orange-t)" />
        <path d="M10 8.5v7M14 8.5v7" stroke="#fff" strokeWidth="1.8" strokeLinecap="round" />
      </svg>
    );
  return (
    <svg {...common}>
      <circle cx="12" cy="12" r="10" fill="var(--brand)" />
      <path d="M12 7v5l3.5 2.2" stroke="#fff" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
};

const LinkIcon = () => (
  <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="var(--tone-blue-t)" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d="M10 13a5 5 0 0 0 7 0l3-3a5 5 0 0 0-7-7l-1.5 1.5" />
    <path d="M14 11a5 5 0 0 0-7 0l-3 3a5 5 0 0 0 7 7l1.5-1.5" />
  </svg>
);

const VestingIcon = () => (
  <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="var(--gv-muted)" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <rect x="3" y="6" width="18" height="13" rx="2" />
    <path d="M3 10h18M7 15h3" />
  </svg>
);

const Chevron = ({ open }: { open: boolean }) => (
  <svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="var(--gv-faint)" strokeWidth="1.6" aria-hidden="true" className={"gpd__chev" + (open ? " is-open" : "")}>
    <circle cx="12" cy="12" r="9.2" />
    <path d="M10 8.5l4 3.5-4 3.5" strokeLinecap="round" strokeLinejoin="round" />
  </svg>
);

function humanizeStatus(status: string) {
  return status
    .replace(/_/g, " ")
    .replace(/\b\w/g, (c) => c.toUpperCase());
}

function StatusPill({ status, project, hero }: { status: string; project: Project; hero?: boolean }) {
  const copy = STATUS_PILL[status];
  const text = (typeof copy === "function" ? copy(project) : copy) || humanizeStatus(status);
  return (
    <span className={"gpd__pill gpd__pill--" + status + (hero ? " gpd__pill--hero" : "")}>{text}</span>
  );
}

function FundingSection({ project }: { project: Project }) {
  const f = project.funding;
  return (
    <div className="gpd__funding">
      <div className="gpd__fundbox">
        <div className="gpd__fundhead">
          <span className="gpd__fundtitle">Project Funding: {f.total} {f.token}</span>
          <span className="gpd__funddates">
            Started {f.enactedLabel}, ends {f.endLabel}.
          </span>
          <span className="gpd__fundnext">
            Next vesting step in {f.nextVested.time} {f.nextVested.unit} - ${f.nextVested.amount}.
          </span>
        </div>

        <div className="gpd__vp">
          <div className="gpd__vplabels">
            <span className="gpd__vpvested">
              <b>{f.vestedAmount} {f.token}</b> vested
              <span className="gpd__pct gpd__pct--yellow">{f.vestedPct}%</span>
            </span>
          </div>
          <div className="gpd__vpbar">
            <span className="gpd__vpfill gpd__vpfill--released" style={{ width: f.releasedPct + "%" }} />
            <span className="gpd__vpfill gpd__vpfill--vested" style={{ width: f.vestedPct + "%" }} />
          </div>
          <div className="gpd__vpreleased">
            <span className="gpd__vpreleaseddot" aria-hidden="true" />
            <span>{f.releasedAmount} {f.token} released</span>
          </div>
        </div>
      </div>

      <span className="gpd__sectiontitle">Vesting Contracts</span>
      <div className="gpd__cards gpd__cards--slim">
        {project.vestings.map((v) => (
          <a key={v.id} className="gpd__linkitem" href={v.url} target="_blank" rel="noreferrer">
            <span className="gpd__linktitle">
              <VestingIcon /> {v.label}
            </span>
          </a>
        ))}
      </div>
    </div>
  );
}

function PersonnelRow({ member }: { member: Personnel }) {
  const [open, setOpen] = useState(false);
  return (
    <div className="gpd__expand">
      <div role="button" className="gpd__expandhead" onClick={() => setOpen((p) => !p)}>
        <div>
          <div className="gpd__expandtitle">
            {member.name}
            {member.address ? <span className="gpd__addr"> ({member.address})</span> : null}
          </div>
          <div className="gpd__expandsub">{member.role}</div>
        </div>
        <Chevron open={open} />
      </div>
      {open && <div className="gpd__expandbody">{member.about}</div>}
    </div>
  );
}

function GeneralInfo({ project }: { project: Project }) {
  const card = STATUS_CARD[project.status];
  return (
    <>
      {project.status !== "pending" && card && (
        <div className={"gpd__statuscard gpd__statuscard--" + project.status}>
          <span className="gpd__statusicon">
            <StatusIcon kind={card.icon} />
          </span>
          <span className="gpd__statustext">{card.title}</span>
        </div>
      )}

      <div>
        <span className="gpd__sectiontitle">About</span>
        <div className="gpd__markdown">
          {project.about.split("\n\n").map((para, i) => (
            <p key={i}>{para}</p>
          ))}
        </div>
      </div>

      <div>
        <span className="gpd__sectiontitle">Project Links</span>
        <div className="gpd__cards gpd__cards--slim">
          <a className="gpd__linkitem" href={project.proposal_id ? `/governance/proposals/${project.proposal_id}` : "/governance/proposals"} rel="noreferrer">
            <span className="gpd__linktitle">
              <LinkIcon /> Proposal
            </span>
          </a>
          {project.links.map((l) => (
            <a key={l.id} className="gpd__linkitem" href={l.url} target="_blank" rel="noreferrer">
              <span className="gpd__linktitle">
                <LinkIcon /> {l.label}
              </span>
            </a>
          ))}
        </div>
      </div>

      <div>
        <span className="gpd__sectiontitle">Personnel</span>
        <div className="gpd__cards">
          {project.personnel.map((m) => (
            <PersonnelRow key={m.id} member={m} />
          ))}
        </div>
      </div>
    </>
  );
}

function MilestoneRow({ milestone }: { milestone: Milestone }) {
  const [open, setOpen] = useState(false);
  return (
    <div className="gpd__expand gpd__expand--slim">
      <div role="button" className="gpd__expandhead" onClick={() => setOpen((p) => !p)}>
        <div className="gpd__expandtitle">{milestone.date} - {milestone.title}</div>
        <Chevron open={open} />
      </div>
      {open && <div className="gpd__expandbody">{milestone.description}</div>}
    </div>
  );
}

function MilestonesBody({ project }: { project: Project }) {
  if (!project.milestones || project.milestones.length === 0) {
    return (
      <div className="gpd__emptytab">
        <p>The team hasn't set any milestones yet</p>
      </div>
    );
  }
  return (
    <div className="gpd__cards">
      {project.milestones.map((m) => (
        <MilestoneRow key={m.id} milestone={m} />
      ))}
    </div>
  );
}

function PlaceholderTab({ label }: { label: string }) {
  return (
    <div className="gpd__emptytab">
      <p>No {label.toLowerCase()} to show yet.</p>
    </div>
  );
}

function LoadingView() {
  return (
    <div className="gpd__loading">
      <span className="gpd__spinner" aria-hidden="true" />
    </div>
  );
}

function NotFound() {
  return (
    <div className="gpd__notfound">
      <div className="gpd__nfcode">404</div>
      <p className="gpd__nftitle">Project not found</p>
      <p className="gpd__nfsub">The project you are looking for doesn't exist or was removed.</p>
      <a className="gpd__nfcta" href="/governance/projects">Back to Projects</a>
    </div>
  );
}

export default function GvProjectDetail({
  chrome = true,
  project,
  loading = false,
  notFound = false,
  initialTabId,
}: {
  chrome?: boolean;
  project?: Project;
  loading?: boolean;
  notFound?: boolean;
  initialTabId?: string;
}) {
  const [tab, setTab] = useState<GovernanceNavId>("projects");
  const startIdx = (() => {
    if (!initialTabId) return 0;
    const i = MENU.findIndex((m) => m.id === initialTabId);
    return i >= 0 ? i : 0;
  })();
  const [viewIdx, setViewIdx] = useState(startIdx);
  const active = project && (project.status === "pending" || project.status === "in_progress");

  if (loading) {
    return (
      <GovernanceChromeMaybe chrome={chrome} active={tab} onTab={setTab}>
        <div className="gpd">
          <LoadingView />
        </div>
      </GovernanceChromeMaybe>
    );
  }

  if (notFound || !project) {
    return (
      <GovernanceChromeMaybe chrome={chrome} active={tab} onTab={setTab}>
        <div className="gpd">
          <NotFound />
        </div>
      </GovernanceChromeMaybe>
    );
  }

  const bodyFor = (id: string | undefined) => {
    if (id === "general_info") return <GeneralInfo project={project} />;
    if (id === "milestones") return <MilestonesBody project={project} />;
    if (id === "updates") return <PlaceholderTab label="Updates" />;
    return <PlaceholderTab label="Activity" />;
  };

  return (
    <GovernanceChromeMaybe chrome={chrome} active={tab} onTab={setTab}>
      <div className="gpd__floating">
        <div className="gpd__floatinner">
          <span className="gpd__floattitle">{project.title}</span>
          <StatusPill status={project.status} project={project} />
        </div>
      </div>

      <div className="gpd">
        <div className={"gpd__hero" + (active ? " is-active" : "")}>
          <div className="gpd__herotext">
            <h1 className={"gpd__herotitle" + (active ? " is-active" : "")}>{project.title}</h1>
            <StatusPill status={project.status} project={project} hero />
          </div>
        </div>

        <div className="gpd__cols">
          <aside className="gpd__left" aria-label="Project sections">
            <div className="gpd__sticky">
              {MENU.map((item, idx) => (
                <button
                  key={item.id}
                  type="button"
                  className={"gpd__vtab" + (idx === viewIdx ? " is-active" : "")}
                  onClick={() => setViewIdx(idx)}
                >
                  {item.label}
                  {item.id === "updates" && <Dot />}
                </button>
              ))}
            </div>
          </aside>

          <section className="gpd__content" aria-label="Project details">{bodyFor(MENU[viewIdx]?.id)}</section>

          <aside className="gpd__right" aria-label="Project funding">
            <div className="gpd__sticky">
              {project.funding && <FundingSection project={project} />}
            </div>
          </aside>
        </div>
      </div>
    </GovernanceChromeMaybe>
  );
}
