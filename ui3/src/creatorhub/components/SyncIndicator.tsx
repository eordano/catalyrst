import {
  useSyncSummary,
  type EngineSummary,
  type SyncEngine,
} from "../lib/sync-engine";
import "./syncindicator.css";

type Tone = "ok" | "busy" | "attn" | "warn";
type Glyph = "check" | "dot" | "cloud-off" | "alert" | null;

type Display = { label: string; tone: Tone; spinner: boolean; glyph: Glyph };

function describe(summary: EngineSummary): Display {
  const { overall, pending, online } = summary;
  switch (overall) {
    case "synced":
      return { label: "All changes synced", tone: "ok", spinner: false, glyph: "check" };
    case "editing":
      return { label: "Editing…", tone: "busy", spinner: false, glyph: "dot" };
    case "syncing":
      return { label: "Syncing…", tone: "busy", spinner: true, glyph: null };
    case "pending":
      return {
        label: pending === 1 ? "1 change pending" : `${pending} changes pending`,
        tone: "attn",
        spinner: false,
        glyph: "dot",
      };
    case "offline":
      return {
        label: online ? "Waiting to sync" : "Offline — will sync",
        tone: "attn",
        spinner: false,
        glyph: "cloud-off",
      };
    case "conflict":
      return { label: "Sync conflict — review", tone: "warn", spinner: false, glyph: "alert" };
    case "error":
      return { label: "Sync error", tone: "warn", spinner: false, glyph: "alert" };
    default:
      return { label: "All changes synced", tone: "ok", spinner: false, glyph: "check" };
  }
}

function tooltipFor(summary: EngineSummary): string {
  switch (summary.overall) {
    case "synced":
      return "Saved locally and backed up to the cloud.";
    case "editing":
      return "You have unsaved edits. They save automatically.";
    case "syncing":
      return "Uploading your latest changes to the cloud.";
    case "pending":
      return `${summary.pending} local change${summary.pending === 1 ? "" : "s"} waiting to reach the cloud.`;
    case "offline":
      return "You're offline. Changes are saved locally and will sync when the connection returns.";
    case "conflict":
      return "This scene changed in the cloud since your last edit. Review to choose which version to keep.";
    case "error":
      return "Something went wrong syncing to the cloud. Your local changes are safe; syncing will retry.";
    default:
      return "Saved locally and backed up to the cloud.";
  }
}

function Glyphicon({ glyph }: { glyph: Glyph }) {
  if (glyph === "check") {
    return (
      <svg viewBox="0 0 20 20" width="14" height="14" aria-hidden="true">
        <path
          d="M4.5 10.5l3.2 3.2 7-7.4"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
        />
      </svg>
    );
  }
  if (glyph === "cloud-off") {
    return (
      <svg viewBox="0 0 20 20" width="14" height="14" aria-hidden="true">
        <path
          d="M6 15h8a3 3 0 0 0 .4-5.97A4.5 4.5 0 0 0 6.2 7.2"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.6"
          strokeLinecap="round"
          strokeLinejoin="round"
        />
        <path d="M3.5 3.5l13 13" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />
      </svg>
    );
  }
  if (glyph === "alert") {
    return (
      <svg viewBox="0 0 20 20" width="14" height="14" aria-hidden="true">
        <path
          d="M10 3.2 18 16.5H2L10 3.2Z"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.6"
          strokeLinejoin="round"
        />
        <path d="M10 8v3.6" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" />
        <circle cx="10" cy="14" r="0.95" fill="currentColor" />
      </svg>
    );
  }
  if (glyph === "dot") {
    return <span className="syncind__dot" />;
  }
  return null;
}

type SyncIndicatorViewProps = {
  summary: EngineSummary;
  className?: string;
  onReview?: () => void;
  title?: string;
};

export function SyncIndicatorView({ summary, className, onReview, title }: SyncIndicatorViewProps) {
  const d = describe(summary);
  const cls = `syncind syncind--${d.tone}` + (className ? " " + className : "");
  const tip = title ?? tooltipFor(summary);
  const interactive = onReview && (summary.overall === "conflict" || summary.overall === "error");

  const inner = (
    <>
      <span className="syncind__mark" aria-hidden="true">
        {d.spinner ? <span className="syncind__spinner" /> : <Glyphicon glyph={d.glyph} />}
      </span>
      <span className="syncind__label">{d.label}</span>
    </>
  );

  return (
    <span className="syncind__live" role="status" aria-live="polite">
      {interactive ? (
        <button type="button" className={cls} title={tip} onClick={onReview} aria-label={`${d.label}. ${tip}`}>
          {inner}
        </button>
      ) : (
        <span className={cls} title={tip}>
          {inner}
        </span>
      )}
    </span>
  );
}

type SyncIndicatorProps = {
  engine: SyncEngine;
  className?: string;
  onReview?: () => void;
  title?: string;
};

export default function SyncIndicator({ engine, className, onReview, title }: SyncIndicatorProps) {
  const summary = useSyncSummary(engine);
  return (
    <SyncIndicatorView summary={summary} className={className} onReview={onReview} title={title} />
  );
}
