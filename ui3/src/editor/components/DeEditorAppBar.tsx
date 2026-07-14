import type { ReactNode } from "react";
import { useState } from "react";

import ChModalMobileQRCode from "../../creatorhub/components/ChModalMobileQRCode";
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

interface MenuToggleProps {
  label: string;
  checked: boolean;
  onChange?: () => void;
  disabled?: boolean;
  hint?: string;
}

function MenuToggle({
  label,
  checked,
  onChange,
  disabled = false,
  hint,
}: MenuToggleProps) {
  return (
    <button
      type="button"
      role="menuitemcheckbox"
      aria-checked={checked}
      aria-disabled={disabled || undefined}
      disabled={disabled}
      className="editor-wizard__menu-item"
      onClick={disabled ? undefined : onChange}
      title={disabled ? hint : undefined}
      style={disabled ? { opacity: 0.5, cursor: "default" } : undefined}
    >
      <span
        className={"editor-wizard__menu-check" + (checked ? " is-checked" : "")}
        aria-hidden="true"
      />
      {label}
      {disabled && hint && (
        <span
          style={{ marginLeft: "auto", color: "var(--ink-45)", fontSize: "12px" }}
        >
          {hint}
        </span>
      )}
    </button>
  );
}

export interface PublishOption {
  id: string;
  label: string;
}

export interface DeEditorAppBarProps {
  title: string;
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
    return u.toString();
  } catch {
    return src;
  }
}

type PreviewState = {
  debugger: boolean;
  enableLandscapeTerrains: boolean;
  multiInstance: boolean;
  qr: boolean;
};

export default function DeEditorAppBar({
  title,
  viewportSrc,
  previewSrc = undefined,
  engine = undefined,
  publishOptions = [],
  onExit = undefined,
  onPublish = undefined,
}: DeEditorAppBarProps) {
  const [menu, setMenu] = useState<"preview" | "publish" | null>(null);
  const [preview, setPreview] = useState<PreviewState>({
    debugger: false,
    enableLandscapeTerrains: true,
    multiInstance: false,
    qr: false,
  });

  const chip = !viewportSrc
    ? { label: "Preview only", online: false }
    : engine === "online"
      ? { label: "Online", online: true }
      : engine === "offline"
        ? { label: "Offline", online: false }
        : { label: "Starting\u{2026}", online: false };
  const playerPreviewUrl = previewSrc ?? (viewportSrc ? stripEditorParams(viewportSrc) : undefined);
  const toggleMenu = (which: "preview" | "publish") =>
    setMenu((cur) => (cur === which ? null : which));
  const flip = (key: keyof PreviewState) => () =>
    setPreview((o) => ({ ...o, [key]: !o[key] }));

  return (
    <div className="editor-wizard__appbar" role="group" aria-label="Scene editor actions">
      <span className="editor-wizard__appbar-title" title={title}>
        {title}
      </span>
      <div className="editor-wizard__appbar-actions">
        {onExit && (
          <button type="button" className="editor-wizard__btn" onClick={onExit}>
            <ExitIcon />
            Exit
          </button>
        )}

        <div className="editor-wizard__split">
          <button
            type="button"
            className="editor-wizard__btn editor-wizard__btn--main"
            disabled={!playerPreviewUrl}
            aria-disabled={!playerPreviewUrl}
            onClick={() => {
              if (playerPreviewUrl) window.open(playerPreviewUrl, "_blank", "noopener,noreferrer");
            }}
          >
            <PreviewIcon />
            Preview
          </button>
          <button
            type="button"
            className="editor-wizard__caret"
            aria-label="Preview options"
            aria-expanded={menu === "preview"}
            onClick={() => toggleMenu("preview")}
          >
            <CaretIcon />
          </button>
          {menu === "preview" && (
            <div className="editor-wizard__menu" role="menu">
              <span className="editor-wizard__menu-title">Preview Options</span>
              <MenuToggle
                label="Open Debug Console"
                checked={preview.debugger}
                onChange={flip("debugger")}
                disabled
                hint="Desktop app only"
              />
              <MenuToggle
                label="Enable Landscape Terrains"
                checked={preview.enableLandscapeTerrains}
                onChange={flip("enableLandscapeTerrains")}
                disabled
                hint="Desktop app only"
              />
              <MenuToggle
                label="Multi-Instance"
                checked={preview.multiInstance}
                onChange={flip("multiInstance")}
                disabled
                hint="Desktop app only"
              />
              <div className="editor-wizard__menu-sep" role="separator" />
              <MenuToggle
                label="Show QR Code"
                checked={preview.qr}
                onChange={flip("qr")}
              />
            </div>
          )}
          {preview.qr && (
            <ChModalMobileQRCode
              open
              url={playerPreviewUrl}
              onClose={() => setPreview((o) => ({ ...o, qr: false }))}
            />
          )}
        </div>

        <div className="editor-wizard__split">
          <button
            type="button"
            className="editor-wizard__btn editor-wizard__btn--primary editor-wizard__btn--main"
            onClick={() => onPublish?.()}
          >
            <PublishIcon />
            Publish
          </button>
          <button
            type="button"
            className="editor-wizard__caret editor-wizard__caret--primary"
            aria-label="Publish options"
            aria-expanded={menu === "publish"}
            onClick={() => toggleMenu("publish")}
          >
            <CaretIcon />
          </button>
          {menu === "publish" && (
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

export interface DeEditorControlsBarProps {
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
