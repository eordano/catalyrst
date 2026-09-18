import type { ReactNode } from "react";
import { useRef, useState } from "react";

import { useDismiss } from "../use-dismiss";
import "./deeditorappbar.css";

const ExitIcon = () => (
  <svg viewBox="0 0 20 20" width="16" height="16" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d="M12 4l-6 6 6 6" />
  </svg>
);
const PreviewIcon = () => (
  <svg viewBox="0 0 24 24" width="16" height="16" fill="currentColor" aria-hidden="true">
    <path d="M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20zM9.5 16.5v-9l7 4.5z" />
  </svg>
);
const PublishIcon = () => (
  <svg viewBox="0 0 24 24" width="16" height="16" fill="currentColor" aria-hidden="true">
    <path d="M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20zm-1 17.93A8.01 8.01 0 0 1 4 12c0-.62.08-1.21.21-1.79L9 15v1a2 2 0 0 0 2 2zm6.9-2.54c-.26-.81-1-1.39-1.9-1.39h-1v-3a1 1 0 0 0-1-1H8v-2h2a1 1 0 0 0 1-1V7h2a2 2 0 0 0 2-2v-.41A8 8 0 0 1 17.9 17.39z" />
  </svg>
);
const CaretIcon = () => (
  <svg viewBox="0 0 24 24" width="18" height="18" fill="currentColor" aria-hidden="true">
    <path d="m7 10 5 5 5-5z" />
  </svg>
);

interface PublishOption {
  id: string;
  label: string;
}

interface DeEditorAppBarProps {
  projectTools?: ReactNode;
  children?: ReactNode;
  busy?: boolean;
  exitDisabled?: boolean;
  title: string;
  onRename?: (name: string) => void;
  viewportSrc?: string;
  previewSrc?: string;
  engine?: "connecting" | "online" | "offline";
  publishOptions?: PublishOption[];
  onExit?: () => void;
  onPublish?: (id?: string) => void;
}

function stripEditorParams(src: string): string {
  try {
    const base = typeof window !== "undefined" ? window.location.href : "http://localhost/";
    const u = new URL(src, base);
    u.searchParams.delete("systemScene");
    u.searchParams.delete("editorUi");
    u.searchParams.delete("editorSession");
    return u.toString();
  } catch {
    return src;
  }
}

export default function DeEditorAppBar({
  projectTools,
  children,
  busy = false,
  exitDisabled = false,
  title,
  onRename,
  viewportSrc,
  previewSrc = undefined,
  engine = undefined,
  publishOptions = [],
  onExit = undefined,
  onPublish = undefined,
}: DeEditorAppBarProps) {
  const [menu, setMenu] = useState<"publish" | null>(null);
  const actionsRef = useRef<HTMLDivElement | null>(null);
  useDismiss(menu !== null, actionsRef, () => setMenu(null));
  const chip = !viewportSrc
    ? { label: "Preview only", online: false }
    : engine === "online"
      ? { label: "Online", online: true }
      : engine === "offline"
        ? { label: "Offline", online: false }
        : { label: "Starting\u{2026}", online: false };
  const playerPreviewUrl = previewSrc ?? (viewportSrc ? stripEditorParams(viewportSrc) : undefined);
  const unavailable = busy || (!!viewportSrc && engine !== "online");
  const publishDisabled = unavailable || !onPublish;

  return (
    <div className="editor-wizard__appbar" role="group" aria-label="Scene editor actions">
      {onExit && (
        <button type="button" className="editor-wizard__btn editor-wizard__back" onClick={onExit} disabled={exitDisabled} aria-label="Back to Creator Hub" title="Back to Creator Hub">
          <ExitIcon />
        </button>
      )}
      {onRename ? <input
        key={title}
        className="editor-wizard__appbar-title editor-wizard__title-input"
        aria-label="Scene title"
        title="Rename the scene"
        defaultValue={title}
        maxLength={200}
        onKeyDown={event => {
          if (event.key === "Escape") event.currentTarget.value = title;
          if (event.key === "Enter" || event.key === "Escape") event.currentTarget.blur();
        }}
        onBlur={event => {
          const name = event.currentTarget.value.trim();
          event.currentTarget.value = title;
          if (name && name !== title) onRename(name);
        }}
      /> : <span className="editor-wizard__appbar-title" title={title}>{title}</span>}
      {children && <div className="editor-wizard__navigation">{children}</div>}
      {projectTools}
      <div className="editor-wizard__appbar-actions" ref={actionsRef}>
        <div className="editor-wizard__split">
          <button
            type="button"
            className="editor-wizard__btn"
            disabled={!playerPreviewUrl || unavailable}
            title={unavailable ? "Wait for the scene to be ready" : "Open a preview in a new tab"}
            onClick={() => {
              if (playerPreviewUrl) window.open(playerPreviewUrl, "_blank", "noopener,noreferrer");
            }}
          >
            <PreviewIcon />
            Preview
          </button>
        </div>

        <div className="editor-wizard__split">
          <button
            type="button"
            className={"editor-wizard__btn editor-wizard__btn--primary" + (publishOptions.length > 1 ? " editor-wizard__btn--main" : "")}
            disabled={publishDisabled}
            title={unavailable ? "Wait for the scene to be ready" : undefined}
            onClick={() => onPublish?.()}
          >
            <PublishIcon />
            Publish
          </button>
          {publishOptions.length > 1 && <button
            type="button"
            className="editor-wizard__caret editor-wizard__caret--primary"
            aria-label="Publish options"
            aria-expanded={menu === "publish"}
            disabled={publishDisabled}
            onClick={() => setMenu(current => current === "publish" ? null : "publish")}
          >
            <CaretIcon />
          </button>}
          {menu === "publish" && !publishDisabled && (
            <div className="editor-wizard__menu" role="menu">
              {publishOptions.map((opt) => (
                <button
                  key={opt.id}
                  type="button"
                  className="editor-wizard__menu-item"
                  onClick={() => {
                    setMenu(null);
                    onPublish?.(opt.id);
                  }}
                >
                  {opt.label}
                </button>
              ))}
            </div>
          )}
        </div>

        <span className={"editor-wizard__chip" + (chip.online ? " is-online" : "")}>
          <span className="editor-wizard__chip-dot" />
          {chip.label}
        </span>
      </div>
    </div>
  );
}

interface DeEditorControlsBarProps {
  label: string;
  children?: ReactNode;
}

export function DeEditorControlsBar({ label, children }: DeEditorControlsBarProps) {
  return (
    <div className="editor-wizard__controls" role="group" aria-label={label}>
      <span className="editor-wizard__steplabel">{label}</span>
      <div className="editor-wizard__actions">{children}</div>
    </div>
  );
}
