import { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import Button from "../../atoms/Button";
import "./permissiondialog.css";

export type PermissionLevelChoice = "once" | "scene" | "realm" | "global";

export type PermissionDialogRequest = {
  id: number;
  ty: string;
  scene: string;
  additional?: string | null;
};

// PermissionType (serde enum name) -> the "wants permission to {...}" clause. Mirrors the engine's
// PermissionStrings::passive (bridge_protocol). Unknown types get a generic fallback.
const PASSIVE: Record<string, string> = {
  MovePlayer: "move your avatar within the scene bounds",
  ForceCamera: "temporarily change the camera view",
  PlayEmote: "make your avatar perform an emote",
  SetLocomotion: "temporarily modify your avatar's locomotion settings",
  HideAvatarsNametags: "temporarily hide player avatars and/or nametags, and/or disables passports",
  DisableVoice: "temporarily disable voice chat",
  Teleport: "teleport you to a new location",
  ChangeRealm: "move you to a new realm",
  SpawnPortable: "spawn a portable experience",
  KillPortables: "manage your active portable experiences",
  Web3: "initiate a web3 transaction with your wallet",
  CopyToClipboard: "copy text into the clipboard",
  Fetch: "fetch data from a remote server",
  Websocket: "open a web socket to communicate with a remote server",
  OpenUrl: "open a url in your browser",
};

const LEVELS: { value: PermissionLevelChoice; label: string }[] = [
  { value: "once", label: "Once" },
  { value: "scene", label: "Always for Scene" },
  { value: "realm", label: "Always for Realm" },
  { value: "global", label: "Always for Global" },
];

function LockIcon() {
  return (
    <svg width="26" height="26" viewBox="0 0 24 24" aria-hidden="true" fill="none">
      <rect x="4" y="10" width="16" height="11" rx="2" fill="currentColor" />
      <path d="M8 10V7a4 4 0 0 1 8 0v3" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
    </svg>
  );
}

export default function PermissionDialog({
  request,
  onResolve,
}: {
  request: PermissionDialogRequest;
  onResolve: (allow: boolean, level: PermissionLevelChoice) => void;
}) {
  const [level, setLevel] = useState<PermissionLevelChoice>("once");
  const passive = PASSIVE[request.ty] ?? "perform a restricted action";

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onResolve(false, "once");
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onResolve]);

  return createPortal(
    <div className="pd__scrim" onClick={() => onResolve(false, "once")}>
      <div
        className="pd"
        role="alertdialog"
        aria-modal="true"
        aria-label="Scene permission request"
        onClick={(e) => e.stopPropagation()}
      >
        <span className="pd__icon">
          <LockIcon />
        </span>
        <div className="pd__prompt">
          The scene <span className="pd__scene">{request.scene || "A scene"}</span> wants permission
          to {passive}
        </div>
        {request.additional ? <div className="pd__additional">{request.additional}</div> : null}

        <div className="pd__options" role="radiogroup" aria-label="Apply this decision">
          {LEVELS.map((opt) => (
            <label key={opt.value} className="pd__option">
              <input
                className="pd__input"
                type="radio"
                name="permission-level"
                checked={level === opt.value}
                onChange={() => setLevel(opt.value)}
              />
              <span className={"pd__radio" + (level === opt.value ? " pd__radio--on" : "")}>
                {level === opt.value ? <span className="pd__radio-dot" /> : null}
              </span>
              {opt.label}
            </label>
          ))}
        </div>

        <div className="pd__actions">
          <Button variant="primary" onClick={() => onResolve(true, level)}>
            Allow
          </Button>
          <Button variant="ghost" className="pd__deny" onClick={() => onResolve(false, level)}>
            Deny
          </Button>
        </div>
      </div>
    </div>,
    document.body
  );
}
