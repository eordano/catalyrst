import type { ReactNode } from "react";
import { useState } from "react";
import { ChevronLeft, ChevronDownAlt, Close } from "../../atoms/icons";
import Spinner from "../../atoms/Spinner";
import "./chpublishwizarddeployprogressresult.css";

const MAX_FILE_SIZE_MB = 50;

const GridIcon = ({ size = 16 }: { size?: number }) => (
  <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true">
    <rect x="3" y="3" width="18" height="18" rx="2" stroke="currentColor" strokeWidth="1.6" fill="none" />
    <path d="M3 9h18M3 15h18M9 3v18M15 3v18" stroke="currentColor" strokeWidth="1.6" />
  </svg>
);
const PinIcon = () => (
  <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true">
    <path d="M12 21s7-6.3 7-11a7 7 0 1 0-14 0c0 4.7 7 11 7 11Z" stroke="currentColor" strokeWidth="1.6" fill="none" strokeLinejoin="round" />
    <circle cx="12" cy="10" r="2.4" stroke="currentColor" strokeWidth="1.6" fill="none" />
  </svg>
);
const InfoOutlinedIcon = () => (
  <svg viewBox="0 0 24 24" width="24" height="24" aria-hidden="true">
    <circle cx="12" cy="12" r="9" stroke="currentColor" strokeWidth="1.8" fill="none" />
    <path d="M12 11v5" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" />
    <circle cx="12" cy="7.6" r="1.1" fill="currentColor" />
  </svg>
);
const CheckIcon = () => (
  <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
    <path d="M5 12.5l4.5 4.5L19 7" stroke="currentColor" strokeWidth="2.2" fill="none" strokeLinecap="round" strokeLinejoin="round" />
  </svg>
);
const DeployIcon = () => (
  <svg className="cpd__deploy-icon" viewBox="0 0 25 24" width="22" height="22" aria-hidden="true">
    <path d="M2.84984 21L23.8398 12L2.84984 3L2.83984 10L17.8398 12L2.83984 14L2.84984 21Z" fill="#fff" />
  </svg>
);
const JumpInIcon = () => (
  <svg className="cpd__jumpin-icon" viewBox="0 0 25 25" width="22" height="22" aria-hidden="true">
    <rect x="1.69922" y="0.960938" width="22.5" height="22.5" rx="7.25" stroke="#FCFCFC" strokeOpacity="0.3" strokeWidth="1.5" />
    <path d="M19.6623 11.276L14.9852 6.60121C14.1514 5.76788 12.7483 6.35731 12.7483 7.53617V9.08087C12.7076 9.08087 12.6873 9.08087 12.6466 9.08087H8.21012C7.45772 9.08087 6.84766 9.6703 6.84766 10.4223V13.9792C6.84766 14.7312 7.45772 15.341 8.21012 15.341H12.6263C12.6669 15.341 12.6873 15.341 12.7279 15.341V16.8857C12.7279 18.0646 14.1514 18.654 14.9648 17.8207L19.642 13.1459C20.1707 12.6174 20.1707 11.7841 19.6623 11.276Z" fill="#fff" />
  </svg>
);
const CopyIcon = () => (
  <svg className="cpd__copy-icon" viewBox="0 0 19 19" width="18" height="18" aria-hidden="true">
    <path d="M13.3242 0.960938H4.32422C3.49922 0.960938 2.82422 1.63594 2.82422 2.46094V12.9609H4.32422V2.46094H13.3242V0.960938ZM15.5742 3.96094H7.32422C6.49922 3.96094 5.82422 4.63594 5.82422 5.46094V15.9609C5.82422 16.7859 6.49922 17.4609 7.32422 17.4609H15.5742C16.3992 17.4609 17.0742 16.7859 17.0742 15.9609V5.46094C17.0742 4.63594 16.3992 3.96094 15.5742 3.96094ZM15.5742 15.9609H7.32422V5.46094H15.5742V15.9609Z" fill="#A09BA8" />
  </svg>
);
const SuccessIcon = () => (
  <svg className="cpd__success-icon" viewBox="0 0 85 65" width="80" height="80" aria-hidden="true">
    <path d="M27.6278 50.7046L7.72625 30.803L0.949219 37.5323L27.6278 64.2109L84.8985 6.94024L78.1692 0.210938L27.6278 50.7046Z" fill="url(#cpd-grad)" />
    <defs>
      <linearGradient id="cpd-grad" x1="0.949217" y1="0.210866" x2="89.2301" y2="57.5408" gradientUnits="userSpaceOnUse">
        <stop stopColor="#FFBC5B" />
        <stop offset="0.505208" stopColor="#FF2D55" />
        <stop offset="1" stopColor="#C640CD" />
      </linearGradient>
    </defs>
  </svg>
);
const WarningIcon = ({ className }: { className?: string }) => (
  <svg className={className} viewBox="0 0 82 70" width="30" height="30" aria-hidden="true">
    <path d="M40.7646 14.7L68.5067 62.6316H13.0225L40.7646 14.7ZM40.7646 0L0.238281 70H81.2909L40.7646 0Z" fill="url(#cpd-wgrad)" />
    <path d="M44.4488 51.5789H37.0804V58.9474H44.4488V51.5789Z" fill="url(#cpd-wgrad)" />
    <path d="M44.4488 29.4737H37.0804V47.8947H44.4488V29.4737Z" fill="url(#cpd-wgrad)" />
    <defs>
      <linearGradient id="cpd-wgrad" x1="0.238279" y1="0" x2="91.446" y2="52.2851" gradientUnits="userSpaceOnUse">
        <stop stopColor="#FFBC5B" />
        <stop offset="0.505208" stopColor="#FF2D55" />
        <stop offset="1" stopColor="#C640CD" />
      </linearGradient>
    </defs>
  </svg>
);

type ChipsData = {
  network: string;
  address: string;
  username?: string;
  verified?: boolean;
  parcel?: string;
  role?: string;
};

function ChipsRow({ network, address, username, verified, parcel, role }: ChipsData) {
  return (
    <div className="cpd__chips">
      <div className="cpd__chip cpd__chip--network">{network}</div>
      <div className="cpd__chip cpd__chip--address">{address}</div>
      {parcel ? (
        <div className="cpd__chip cpd__chip--parcel">
          <PinIcon />
          {parcel}
        </div>
      ) : username || verified ? (
        <div className="cpd__chip cpd__chip--username">
          {username}
          {verified ? <i className="cpd__verified" role="img" aria-label="verified" /> : null}
        </div>
      ) : null}
      {role ? <div className="cpd__chip cpd__chip--role">{role}</div> : null}
    </div>
  );
}

type PublishProject = { title: string; size: string; grad: string };

function ProjectInfo({ project }: { project: PublishProject }) {
  return (
    <div className="cpd__info">
      <div className="cpd__projthumb" style={{ background: project.grad }} />
      <div className="cpd__projtext">
        <div className="cpd__projtitle">{project.title}</div>
        <div className="cpd__projparcels">
          <GridIcon size={12} />
          Scene size: {project.size}
        </div>
      </div>
    </div>
  );
}

function Loader() {
  return (
    <span className="cpd__loader" role="status" aria-label="loading">
      <Spinner size={40} aria-hidden />
    </span>
  );
}

type StepState = "idle" | "pending" | "complete" | "failed";
type StepProps = { bulletText?: number | string; name: string; description?: string; state?: StepState };

function Step({ bulletText, name, description, state = "idle" }: StepProps) {
  const bullet: ReactNode = state === "complete" ? <CheckIcon /> : state === "failed" ? <Close size={20} /> : bulletText;
  return (
    <div className={"cpd__step cpd__step--" + state}>
      <div className="cpd__step-bullet">{bullet}</div>
      <div className="cpd__step-body">
        <h3>{name}</h3>
        <span>{description}</span>
      </div>
    </div>
  );
}
function ConnectedSteps({ steps }: { steps: StepProps[] }) {
  return (
    <div className="cpd__steps">
      {steps.map((s, i) => (
        <Step key={i} {...s} />
      ))}
    </div>
  );
}

type CopyHandler = (url: string) => void;

type JumpUrlProps = { inProgress?: boolean; isWorld?: boolean; url: string; onCopy?: CopyHandler };

function JumpUrl({ inProgress, isWorld, url, onCopy }: JumpUrlProps) {
  return (
    <div className="cpd__jump-in-url">
      {inProgress && <label>You can now access your scene even while it's still processing.</label>}
      <label>The URL to jump in your {isWorld ? "World" : "Land"} is:</label>
      <div className="cpd__url">
        {url}
        {onCopy ? (
          <button type="button" className="cpd__copybtn" aria-label="Copy URL" onClick={() => onCopy(url)}>
            <CopyIcon />
          </button>
        ) : null}
      </div>
    </div>
  );
}

type DeployFile = { name: string; size: number };

type IdleProps = { files: DeployFile[]; exceeded?: boolean; onPublish?: () => void };

function Idle({ files, exceeded, onPublish }: IdleProps) {
  const totalBytes = files.reduce((t, f) => t + f.size, 0);
  const errorMessage = exceeded
    ? `One or more assets exceed the allowed limit (${MAX_FILE_SIZE_MB}MB).\nOptimize or remove large files and try again.`
    : "";
  return (
    <div className="cpd__files">
      <div className="cpd__filters">
        <div className="cpd__count">
          {files.length} {files.length === 1 ? "file" : "files"}
        </div>
        <div className="cpd__size">
          Total Size{" "}
          <b>
            {formatSize(totalBytes)}/{MAX_FILE_SIZE_MB}MB
          </b>
        </div>
      </div>
      <div className="cpd__list">
        {files.map((file) => (
          <div
            className={"cpd__file" + (file.size > MAX_FILE_SIZE_MB * 1e6 ? " is-invalid" : "")}
            key={file.name}
          >
            <div className="cpd__filename" title={file.name}>
              {file.name}
            </div>
            <div className="cpd__filesize">{formatSize(file.size)}</div>
          </div>
        ))}
      </div>
      <div className="cpd__files-actions">
        <p className="cpd__error">{errorMessage}</p>
        {onPublish ? (
          <button
            type="button"
            className="cpd__btn cpd__btn--primary cpd__btn--lg"
            disabled={exceeded}
            onClick={onPublish}
          >
            Publish
            <DeployIcon />
          </button>
        ) : null}
      </div>
    </div>
  );
}

type DeployingProps = {
  steps: StepProps[];
  finishing?: boolean;
  isWorld?: boolean;
  url: string;
  onJumpIn?: () => void;
  onCopy?: CopyHandler;
};

function Deploying({ steps, finishing, isWorld, url, onJumpIn, onCopy }: DeployingProps) {
  return (
    <div className="cpd__deploying">
      <div className="cpd__header">
        <Loader />
        <span className="cpd__h5">{finishing ? "Finishing up..." : "Publishing..."}</span>
      </div>
      <ConnectedSteps steps={steps} />
      {finishing ? (
        <>
          <div className="cpd__jump">
            <JumpUrl inProgress isWorld={isWorld} url={url} onCopy={onCopy} />
          </div>
          {onJumpIn ? (
            <div className="cpd__actions">
              <button type="button" className="cpd__btn cpd__btn--primary cpd__btn--lg" onClick={onJumpIn}>
                Jump In
                <JumpInIcon />
              </button>
            </div>
          ) : null}
        </>
      ) : (
        <div className="cpd__deploy-info">
          <InfoOutlinedIcon />
          Publishing may take up to 15 minutes. During this time, your scene may appear empty
          until it has been updated in the client.
        </div>
      )}
    </div>
  );
}

const XIcon = () => (
  <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
    <path d="M18.244 2.25h3.308l-7.227 8.26 8.502 11.24H16.17l-5.214-6.817L4.99 21.75H1.68l7.73-8.835L1.254 2.25H8.08l4.713 6.231 5.45-6.231Zm-1.161 17.52h1.833L7.084 4.126H5.117L17.083 19.77Z" fill="currentColor" />
  </svg>
);
const FacebookIcon = () => (
  <svg viewBox="0 0 24 24" width="19" height="19" aria-hidden="true">
    <path d="M22 12.06C22 6.5 17.52 2 12 2S2 6.5 2 12.06c0 5.02 3.66 9.18 8.44 9.94v-7.03H7.9v-2.9h2.54V9.85c0-2.52 1.5-3.91 3.78-3.91 1.1 0 2.24.2 2.24.2v2.47h-1.26c-1.24 0-1.63.78-1.63 1.57v1.88h2.78l-.44 2.9h-2.34V22c4.78-.76 8.44-4.92 8.44-9.94Z" fill="currentColor" />
  </svg>
);
const FarcasterIcon = () => (
  <svg viewBox="0 0 24 24" width="19" height="19" aria-hidden="true">
    <path d="M6 3h12v2.4h-1.9v13.2H18V21h-5.1v-2.4h1.3V9.6a2.2 2.2 0 0 0-4.4 0v9H11V21H6v-2.4h1.9V5.4H6V3Z" fill="currentColor" />
  </svg>
);

type SuccessProps = {
  isWorld?: boolean;
  url: string;
  onJumpIn?: () => void;
  onCopy?: CopyHandler;
  onKeepEditing?: () => void;
};

function Success({ isWorld, url, onJumpIn, onCopy, onKeepEditing }: SuccessProps) {
  const [copied, setCopied] = useState(false);
  const enc = encodeURIComponent;
  const shareText = `I just published a ${isWorld ? "world" : "scene"} on Decentraland \u{2014} come hang out.`;
  const shareX = `https://twitter.com/intent/tweet?text=${enc(shareText)}&url=${enc(url)}`;
  const shareFb = `https://www.facebook.com/sharer/sharer.php?u=${enc(url)}`;
  const shareFc = `https://warpcast.com/~/compose?text=${enc(`${shareText} ${url}`)}`;

  const copy = () => {
    onCopy?.(url);
    setCopied(true);
    if (typeof window !== "undefined") window.setTimeout(() => setCopied(false), 2000);
  };

  return (
    <div className="cpd__success">
      <div className="cpd__success-hero">
        <SuccessIcon />
        <div className="cpd__success-message">Your {isWorld ? "world" : "scene"} is live</div>
        <p className="cpd__success-sub">
          It&apos;s published and open right now &#x2014; jump in, or share it with the world.
        </p>
      </div>

      <div className="cpd__sharebar">
        <span className="cpd__sharebar-url" title={url}>
          {url}
        </span>
        <button
          type="button"
          className={"cpd__sharebar-copy" + (copied ? " is-copied" : "")}
          onClick={copy}
        >
          {copied ? <CheckIcon /> : <CopyIcon />}
          {copied ? "Copied" : "Copy link"}
        </button>
      </div>

      {onJumpIn ? (
        <button
          type="button"
          className="cpd__btn cpd__btn--primary cpd__btn--lg cpd__jumpbtn"
          onClick={onJumpIn}
        >
          Jump in
          <JumpInIcon />
        </button>
      ) : null}

      <div className="cpd__next">
        <span className="cpd__next-label">Share it</span>
        <div className="cpd__next-socials">
          <a className="cpd__social" href={shareX} target="_blank" rel="noopener noreferrer" aria-label="Share on X">
            <XIcon />
          </a>
          <a className="cpd__social" href={shareFb} target="_blank" rel="noopener noreferrer" aria-label="Share on Facebook">
            <FacebookIcon />
          </a>
          <a className="cpd__social" href={shareFc} target="_blank" rel="noopener noreferrer" aria-label="Share on Farcaster">
            <FarcasterIcon />
          </a>
        </div>
      </div>

      {onKeepEditing ? (
        <button type="button" className="cpd__keepediting" onClick={onKeepEditing}>
          <ChevronLeft size={16} />
          Keep editing this scene
        </button>
      ) : null}
    </div>
  );
}

function ExpandMore({ title, text }: { title?: string; text?: string }) {
  const [expanded, setExpanded] = useState(false);
  return (
    <div className="cpd__expand">
      <div className="cpd__expand-head">
        <button
          type="button"
          className={"cpd__expand-btn" + (expanded ? " is-open" : "")}
          aria-expanded={expanded}
          aria-label={title || "show more"}
          onClick={() => setExpanded((v) => !v)}
        >
          <ChevronDownAlt size={22} />
        </button>
        {title && <span className="cpd__expand-title">{title}</span>}
      </div>
      {expanded && <p className="cpd__expand-text">{text}</p>}
    </div>
  );
}

type ErrorStateProps = {
  message: string;
  cause?: string;
  steps: StepProps[];
  onRetry?: () => void;
  onReport?: () => void;
};

function ErrorState({ message, cause, steps, onRetry, onReport }: ErrorStateProps) {
  return (
    <div className="cpd__error-state">
      <div className="cpd__header">
        <WarningIcon className="cpd__header-warning" />
        <span className="cpd__h5">Publishing failed</span>
      </div>
      <p className="cpd__error-description">{message}</p>
      {cause && <ExpandMore title="Details" text={cause} />}
      <ConnectedSteps steps={steps} />
      {onReport || onRetry ? (
        <div className="cpd__actions cpd__actions--split">
          {onReport ? (
            <button type="button" className="cpd__btn cpd__btn--secondary cpd__btn--lg" onClick={onReport}>
              Report an issue
            </button>
          ) : null}
          {onRetry ? (
            <button type="button" className="cpd__btn cpd__btn--primary cpd__btn--lg" onClick={onRetry}>
              Retry
            </button>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}

const CHIPS: ChipsData = {
  network: "Mainnet",
  address: "0x9f3c\u{2026}7a21",
  parcel: "12,42",
  role: "Owner",
};

const PROJECT: PublishProject = {
  title: "Neon Night Market",
  size: "2x2",
  grad: "linear-gradient(135deg, #ff2d55 0%, #350447 100%)",
};

const FILES: DeployFile[] = [
  { name: "bin/index.js", size: 1_842_133 },
  { name: "scene.json", size: 1_204 },
  { name: "assets/scene/models/market-stall.glb", size: 8_930_512 },
  { name: "assets/scene/models/neon-sign.glb", size: 3_204_771 },
  { name: "assets/scene/textures/asphalt_diffuse.png", size: 2_118_440 },
  { name: "assets/scene/textures/asphalt_normal.png", size: 1_980_022 },
  { name: "assets/scene/audio/ambient-city.mp3", size: 4_512_900 },
  { name: "assets/scene/models/crowd-character.glb", size: 12_770_310 },
  { name: "images/scene-thumbnail.png", size: 312_889 },
];

const FILES_EXCEEDED: DeployFile[] = [
  ...FILES.slice(0, 6),
  { name: "assets/scene/video/intro-loop-4k.mp4", size: 63_882_104 },
  ...FILES.slice(6),
];

const STEPS_PENDING: StepProps[] = [
  {
    bulletText: 1,
    name: "Publishing",
    description: "Uploading your scene to the Worlds server...",
    state: "pending",
  },
];
const STEPS_FINISHING: StepProps[] = [
  {
    bulletText: 1,
    name: "Publishing",
    description: "The Worlds server accepted your scene.",
    state: "complete",
  },
];
const STEPS_FAILED: StepProps[] = [
  {
    bulletText: 1,
    name: "Publishing",
    description: "The upload was rejected or failed.",
    state: "failed",
  },
];

const JUMP_URL = "decentraland://?position=12,42&dclenv=org";

type DeployError = { message: string; cause?: string };

const ERROR: DeployError = {
  message: "Publishing to the content network failed.\nPlease retry or check your internet connection.",
  cause:
    "Error: request to https://peer.decentraland.org/content/entities failed\n" +
    "  reason: connect ETIMEDOUT 104.18.x.x:443\n" +
    "    at ClientRequest.<anonymous> (deploy.ts:214:18)\n" +
    "    at TLSSocket.emit (node:events:519:28)",
};

function formatSize(size: number): string {
  const KB = 1e3;
  const MB = 1e6;
  const GB = 1e9;
  if (size < KB) return `${size.toFixed(2)} B`;
  if (size < MB) return `${(size / KB).toFixed(2)} KB`;
  if (size < GB) return `${(size / MB).toFixed(2)} MB`;
  return `${(size / GB).toFixed(2)} GB`;
}

type DeployStateValue = "idle" | "exceeded" | "deploying" | "finishing" | "complete" | "error";

type ChPublishWizardDeployProgressResultProps = {
  state?: DeployStateValue;
  chips?: ChipsData;
  project?: PublishProject;
  files?: DeployFile[];
  url?: string;
  isWorld?: boolean;
  steps?: StepProps[];
  error?: DeployError;
  sample?: boolean;
  inline?: boolean;
  onBack?: () => void;
  onClose?: () => void;
  onPublish?: () => void;
  onJumpIn?: () => void;
  onCopy?: CopyHandler;
  onKeepEditing?: () => void;
  onRetry?: () => void;
  onReport?: () => void;
};

export default function ChPublishWizardDeployProgressResult({
  state = "idle",
  chips = undefined,
  project = undefined,
  files = undefined,
  url = undefined,
  isWorld = false,
  steps = undefined,
  error = undefined,
  sample = undefined,
  inline = true,
  onBack = undefined,
  onClose = undefined,
  onPublish = undefined,
  onJumpIn = undefined,
  onCopy = undefined,
  onKeepEditing = undefined,
  onRetry = undefined,
  onReport = undefined,
}: ChPublishWizardDeployProgressResultProps) {
  const isComplete = state === "complete";
  const isError = state === "error";
  const isExceeded = state === "exceeded";

  const isSample = sample !== undefined ? sample : !project && !files && !url;

  const resolvedChips = chips || (isSample ? CHIPS : undefined);
  const resolvedProject = project || (isSample ? PROJECT : undefined);
  const resolvedFiles = files || (isExceeded ? FILES_EXCEEDED : FILES);
  const resolvedUrl = url || JUMP_URL;
  const resolvedSteps =
    steps || (state === "finishing" ? STEPS_FINISHING : isError ? STEPS_FAILED : STEPS_PENDING);
  const resolvedError = error || ERROR;

  const title =
    state === "deploying" || state === "finishing"
      ? "Publishing..."
      : isComplete
        ? `Published to your ${isWorld ? "World" : "Land"}`
        : `Publish to your ${isWorld ? "World" : "Land"}`;

  const panel = (
    <div
      className={"cpd__modal" + (inline ? " cpd__modal--inline" : "")}
      {...(inline ? {} : ({ role: "dialog", "aria-modal": "true", "aria-label": "Publish" } as const))}
    >
      <header className="cpd__header-bar">
        {onBack && !isComplete ? (
          <button type="button" className="cpd__iconbtn cpd__back" aria-label="back" onClick={onBack}>
            <ChevronLeft size={22} />
          </button>
        ) : null}
        <h2 className="cpd__titletext">{title}</h2>
        {onClose ? (
          <button type="button" className="cpd__iconbtn cpd__close" aria-label="close" onClick={onClose}>
            <Close size={20} />
          </button>
        ) : null}
      </header>

      {isSample ? (
        <div className="cpd__sample-ribbon">Sample preview &#x2014; not your scene</div>
      ) : null}

      <div className="cpd__body">
        <div className="cpd__stepwrapper">
          {resolvedChips ? <ChipsRow {...resolvedChips} /> : null}
          <div className="cpd__projcontainer">
            {resolvedProject ? <ProjectInfo project={resolvedProject} /> : null}
            <div className="cpd__content cpd__deploy">
              {(state === "idle" || isExceeded) && (
                <Idle files={resolvedFiles} exceeded={isExceeded} onPublish={onPublish} />
              )}
              {(state === "deploying" || state === "finishing") && (
                <Deploying
                  steps={resolvedSteps}
                  finishing={state === "finishing"}
                  isWorld={isWorld}
                  url={resolvedUrl}
                  onJumpIn={onJumpIn}
                  onCopy={onCopy}
                />
              )}
              {isComplete && (
                <Success
                  isWorld={isWorld}
                  url={resolvedUrl}
                  onJumpIn={onJumpIn}
                  onCopy={onCopy}
                  onKeepEditing={onKeepEditing}
                />
              )}
              {isError && (
                <ErrorState
                  message={resolvedError.message}
                  cause={resolvedError.cause}
                  steps={resolvedSteps}
                  onRetry={onRetry}
                  onReport={onReport}
                />
              )}
            </div>
          </div>
        </div>
      </div>
    </div>
  );

  return inline ? panel : <div className="cpd__backdrop">{panel}</div>;
}
