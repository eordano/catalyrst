import type { ReactNode } from "react";
import { useEffect, useState } from "react";
import { asset } from "../../asset";
import { ChevronLeft, ChevronDownAlt, ChevronRight, Close } from "../../atoms/icons";
import "./chpublishwizardpublishtoworld.css";

const LayersIcon = ({ size = 20 }: { size?: number }) => (
  <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true">
    <path d="M12 3l9 5-9 5-9-5 9-5Z" stroke="currentColor" strokeWidth="1.6" fill="none" strokeLinejoin="round" />
    <path d="M3 12l9 5 9-5M3 16l9 5 9-5" stroke="currentColor" strokeWidth="1.6" fill="none" strokeLinejoin="round" />
  </svg>
);
const GridIcon = ({ size = 16 }: { size?: number }) => (
  <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true">
    <rect x="3" y="3" width="18" height="18" rx="2" stroke="currentColor" strokeWidth="1.6" fill="none" />
    <path d="M3 9h18M3 15h18M9 3v18M15 3v18" stroke="currentColor" strokeWidth="1.6" />
  </svg>
);
const PlusIcon = () => (
  <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
    <path d="M12 5v14M5 12h14" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
  </svg>
);
const LogoDCL = () => (
  <span className="cpw__provlogo" aria-hidden="true">
    <img src={asset("assets/dcl-logo.png")} alt="" />
  </span>
);
const EmptyWorldArt = () => (
  <svg className="cpw__emptyart" viewBox="0 0 220 220" width="220" height="220" aria-hidden="true">
    <defs>
      <radialGradient id="cpw-globe" cx="38%" cy="34%" r="75%">
        <stop offset="0" stopColor="#4b6bff" />
        <stop offset="0.55" stopColor="#7a3df0" />
        <stop offset="1" stopColor="#2a0c52" />
      </radialGradient>
    </defs>
    <ellipse cx="110" cy="186" rx="74" ry="13" fill="rgba(0,0,0,0.35)" />
    <circle cx="110" cy="100" r="78" fill="url(#cpw-globe)" />
    <g stroke="rgba(255,255,255,0.45)" strokeWidth="1.4" fill="none">
      <ellipse cx="110" cy="100" rx="78" ry="30" />
      <ellipse cx="110" cy="100" rx="78" ry="58" />
      <path d="M110 22v156M48 60q62 40 124 0M48 140q62-40 124 0" />
    </g>
    <g fill="rgba(255,255,255,0.18)">
      <path d="M74 64q14-8 26 2 8 6 2 16-10 8-22 2-12-8-6-22Z" />
      <path d="M126 110q16-4 22 8 4 12-8 18-14 4-20-8-4-12 6-18Z" />
    </g>
    <circle cx="84" cy="58" r="4.5" fill="#ff7439" />
    <circle cx="148" cy="132" r="4.5" fill="#34ce76" />
  </svg>
);

type Owner = {
  network?: string;
  address?: string;
  username?: string;
  verified?: boolean;
  role?: string;
};
type Project = { title?: string; size?: string; grad?: string };
type World = { title: string; scenes: number; size: string; grad: string };
type WizardState = "selection" | "empty" | "signedOut";

function ChipsRow({ network, address, username, verified, role }: Owner) {
  return (
    <div className="cpw__chips">
      {network ? <div className="cpw__chip cpw__chip--network">{network}</div> : null}
      {address ? <div className="cpw__chip cpw__chip--address">{address}</div> : null}
      {username ? (
        <div className="cpw__chip cpw__chip--username">
          {username}
          {verified ? <i className="cpw__verified" role="img" aria-label="verified" /> : null}
        </div>
      ) : null}
      {role ? <div className="cpw__chip cpw__chip--role">{role}</div> : null}
    </div>
  );
}

function ProjectInfo({ project }: { project: Project }) {
  return (
    <div className="cpw__info">
      <div className="cpw__projthumb" style={{ background: project.grad }} />
      <div className="cpw__projtext">
        <div className="cpw__projtitle">{project.title}</div>
        {project.size ? (
          <div className="cpw__projparcels">
            <GridIcon size={12} />
            Scene size: {project.size}
          </div>
        ) : null}
      </div>
    </div>
  );
}

function InfoItem({ icon, label }: { icon: ReactNode; label?: string }) {
  if (!label) return null;
  return (
    <div className="cpw__worldinfoitem">
      {icon}
      <span>{label}</span>
    </div>
  );
}

function ClaimNote({ note }: { note?: string }) {
  if (!note) return null;
  return (
    <p className="cpw__claimnote" role="note">
      {note}
    </p>
  );
}

type SelectWorldData = { name?: string; names?: string[]; world?: World | null };

type SelectWorldProps = {
  data: SelectWorldData;
  multiScene: boolean;
  claimNote?: string;
  onToggleMulti: (value: boolean) => void;
  onPickName?: (name: string) => void;
  onReview?: () => void;
  onClaimName?: () => void;
};

function SelectWorld({ data, multiScene, claimNote, onToggleMulti, onPickName, onReview, onClaimName }: SelectWorldProps) {
  const { name, world, names } = data;
  const hasName = !!name;
  const [open, setOpen] = useState(false);
  const showMenu = (open || !hasName) && names && names.length > 0;
  return (
    <div className="cpw__selectworld">
      <div className="cpw__selection">
        <p className="cpw__desc">Choose the domain where your World will be published.</p>
        <div className="cpw__selectrow">
          <button type="button" className="cpw__select cpw__select--provider">
            <span className="cpw__selectval">
              <LogoDCL />
              NAME
            </span>
            <ChevronDownAlt size={20} />
          </button>
          <button
            type="button"
            className="cpw__select cpw__select--name"
            aria-expanded={showMenu}
            onClick={() => setOpen((v) => !v)}
          >
            <span className={"cpw__selectval" + (hasName ? "" : " cpw__placeholder")}>
              {hasName ? name : "Select a Name"}
            </span>
            <ChevronDownAlt size={20} />
          </button>
        </div>
        {showMenu ? (
          <div className="cpw__menu" role="listbox" aria-label="World names">
            {names?.map((n) => (
              <div
                key={n}
                className="cpw__menuitem"
                role="option"
                aria-selected={n === name}
                onClick={() => {
                  onPickName && onPickName(n);
                  setOpen(false);
                }}
              >
                {n}
              </div>
            ))}
            <div
              className="cpw__menuitem cpw__menuitem--claim"
              role="option"
              onClick={() => onClaimName && onClaimName()}
            >
              <PlusIcon /> Claim a new NAME
            </div>
            <ClaimNote note={claimNote} />
          </div>
        ) : null}
      </div>

      {hasName ? (
        <div className="cpw__advanced">
          <div className="cpw__advrow">
            <LayersIcon />
            <div className="cpw__advtexts">
              <div className="cpw__advtitle">Multi-Scene World (Advanced)</div>
              <div className="cpw__advdesc">
                Allow your world to contain more than 1 scene. Total world size will
                depend on your published world layout.
              </div>
            </div>
            <button
              type="button"
              role="switch"
              aria-checked={multiScene}
              aria-label="Multi-Scene World (Advanced)"
              className={"cpw__switch" + (multiScene ? " is-on" : "")}
              onClick={() => onToggleMulti(!multiScene)}
            >
              <span className="cpw__switchknob" />
            </button>
          </div>

          {multiScene && world ? (
            <div className="cpw__worldinfo">
              <div className="cpw__worldthumb" style={{ background: world.grad }} />
              <div className="cpw__worldcontent">
                <div className="cpw__currentlabel">Current World</div>
                <div className="cpw__worldtitle">{world.title}</div>
                <div className="cpw__worldmeta">
                  <InfoItem icon={<LayersIcon size={16} />} label={`${world.scenes} ${world.scenes === 1 ? "scene" : "scenes"}`} />
                  <InfoItem icon={<GridIcon size={16} />} label={world.size} />
                </div>
              </div>
            </div>
          ) : null}
        </div>
      ) : null}

      <div className="cpw__actions">
        <button
          type="button"
          className="cpw__btn cpw__btn--primary"
          disabled={!hasName}
          onClick={onReview}
        >
          {multiScene && world ? "Select Location" : "Review"}
          <ChevronRight size={18} />
        </button>
      </div>
    </div>
  );
}

function SignedOutGate({
  claimNote,
  onSignIn,
  onClaimName,
}: {
  claimNote?: string;
  onSignIn?: () => void;
  onClaimName?: () => void;
}) {
  return (
    <div className="cpw__empty">
      <h3 className="cpw__emptytitle">Sign in to publish to your World</h3>
      <EmptyWorldArt />
      <p className="cpw__emptybody">
        <b>Worlds are tied to the NAMEs your account owns</b>.<br />
        Sign in to see your available Worlds — you may already have one. Every
        NAME you own comes with a free World; a new NAME costs 100 MANA. World
        storage grows with your holdings: 100 Mb per 2,000 MANA, per LAND, and
        per NAME.
      </p>
      <div className="cpw__emptyactions">
        <button
          type="button"
          className="cpw__btn cpw__btn--primary cpw__btn--block"
          onClick={onSignIn}
        >
          Sign in
        </button>
        {onClaimName ? (
          <button
            type="button"
            className="cpw__btn cpw__btn--secondary cpw__btn--block"
            onClick={onClaimName}
          >
            Claim a new NAME
          </button>
        ) : null}
        <ClaimNote note={claimNote} />
      </div>
    </div>
  );
}

function PendingName({
  pendingName,
  onRefresh,
}: {
  pendingName: string;
  onRefresh?: () => void;
}) {
  return (
    <div className="cpw__empty" role="status">
      <h3 className="cpw__emptytitle">Your NAME is on its way</h3>
      <EmptyWorldArt />
      <p className="cpw__emptybody">
        <b>{pendingName}</b> was just claimed.
        <br />
        Newly claimed NAMEs can take a few minutes to be indexed before their
        World shows up here — no need to claim another one.
      </p>
      <div className="cpw__emptyactions">
        {onRefresh ? (
          <button
            type="button"
            className="cpw__btn cpw__btn--primary cpw__btn--block"
            onClick={onRefresh}
          >
            Check again
          </button>
        ) : null}
      </div>
    </div>
  );
}

function EmptyNames({
  claimNote,
  onClaimName,
}: {
  claimNote?: string;
  onClaimName?: () => void;
}) {
  return (
    <div className="cpw__empty">
      <h3 className="cpw__emptytitle">You don't have any available World</h3>
      <EmptyWorldArt />
      <p className="cpw__emptybody">
        <b>Get a free world when you own a NAME</b>.<br />
        Each NAME (100 MANA) will give you access to one World. You can have as
        many as you want.
      </p>
      <div className="cpw__emptyactions">
        <button type="button" className="cpw__btn cpw__btn--primary cpw__btn--block" onClick={onClaimName}>Claim a new Name</button>
        <ClaimNote note={claimNote} />
      </div>
    </div>
  );
}

type ChPublishWizardPublishToWorldProps = {
  state?: WizardState;
  project?: Project;
  owner?: Owner;
  names?: string[];
  selectedName?: string;
  world?: World | null;
  multiScene?: boolean;
  inline?: boolean;
  pendingName?: string;
  claimNote?: string;
  onToggleMulti?: (value: boolean) => void;
  onPickName?: (name: string) => void;
  onReview?: () => void;
  onBack?: () => void;
  onClose?: () => void;
  onClaimName?: () => void;
  onSignIn?: () => void;
  onRefresh?: () => void;
};

export default function ChPublishWizardPublishToWorld({
  state = "selection",
  project = {},
  owner = {},
  names = [],
  selectedName = undefined,
  world = null,
  multiScene = undefined,
  inline = false,
  pendingName = undefined,
  claimNote = undefined,
  onToggleMulti = undefined,
  onPickName = undefined,
  onReview = undefined,
  onBack = undefined,
  onClose = undefined,
  onClaimName = undefined,
  onSignIn = undefined,
  onRefresh = undefined,
}: ChPublishWizardPublishToWorldProps) {
  const [multiSceneState, setMultiSceneState] = useState(true);
  const multiSceneOn = multiScene ?? multiSceneState;
  const toggleMulti = onToggleMulti ?? setMultiSceneState;
  const isEmpty = state === "empty";
  const isSignedOut = state === "signedOut";

  useEffect(() => {
    if (inline || !onClose) return undefined;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [inline, onClose]);

  const panel = (
    <div
      className={
        "cpw__modal" +
        (isEmpty || isSignedOut ? " cpw__modal--empty" : "") +
        (inline ? " cpw__modal--inline" : "")
      }
      {...(inline
        ? {}
        : ({ role: "dialog", "aria-modal": "true", "aria-label": "Publish to your World" } as const))}
    >
      <header className="cpw__header">
        {onBack ? (
          <button type="button" className="cpw__iconbtn cpw__back" aria-label="back" onClick={onBack}><ChevronLeft size={22} /></button>
        ) : null}
        <h2 className="cpw__titletext">{isEmpty ? "Worlds" : "Publish to your World"}</h2>
        {onClose ? (
          <button type="button" className="cpw__iconbtn cpw__close" aria-label="close" onClick={onClose}><Close size={20} /></button>
        ) : null}
      </header>

      <div className="cpw__body">
        {isSignedOut ? (
          <SignedOutGate
            claimNote={claimNote}
            onSignIn={onSignIn}
            onClaimName={onClaimName}
          />
        ) : isEmpty ? (
          pendingName ? (
            <PendingName pendingName={pendingName} onRefresh={onRefresh} />
          ) : (
            <EmptyNames claimNote={claimNote} onClaimName={onClaimName} />
          )
        ) : (
          <div className="cpw__stepwrapper">
            <ChipsRow {...owner} />
            {pendingName ? (
              <div className="cpw__pendingnote" role="status">
                <span>
                  <b>{pendingName}</b> was just claimed — it will appear in this
                  list once it finishes indexing.
                </span>
                {onRefresh ? (
                  <button
                    type="button"
                    className="cpw__pendingrefresh"
                    onClick={onRefresh}
                  >
                    Check again
                  </button>
                ) : null}
              </div>
            ) : null}
            <div className="cpw__projcontainer">
              <ProjectInfo project={project} />
              <div className="cpw__content">
                <SelectWorld
                  data={{ name: selectedName, names, world }}
                  multiScene={multiSceneOn}
                  claimNote={claimNote}
                  onToggleMulti={toggleMulti}
                  onPickName={onPickName}
                  onReview={onReview}
                  onClaimName={onClaimName}
                />
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );

  if (inline) return panel;

  return (
    <div
      className="cpw__backdrop"
      onClick={
        onClose
          ? (e) => {
              if (e.target === e.currentTarget) onClose();
            }
          : undefined
      }
    >
      {panel}
    </div>
  );
}
