import { useCallback, useEffect, useState } from "react";
import "./camera.css";
import { sendBridge, subscribeBridge, useBridgeState } from "../../overlay/bridge";
import { serviceBase, signedFetch } from "../../data/catalyst/client";
import Lightbox from "../components/Lightbox";

type Shortcut = { action: string; keys: string[] };

const SHORTCUTS: Shortcut[] = [
  { action: "Take a photo", keys: ["Space"] },
  { action: "Move camera", keys: ["W", "A", "S", "D"] },
  { action: "Up / Down", keys: ["Q", "/", "E"] },
  { action: "Rotate", keys: ["Right Mouse"] },
  { action: "Zoom", keys: ["Scroll"] },
  { action: "Adjust speed", keys: ["Shift"] },
  { action: "Roll camera", keys: [",", "/", "."] },
  { action: "Reset roll", keys: ["R"] },
  { action: "Toggle UI", keys: ["H"] },
  { action: "Exit camera", keys: ["Esc"] },
];

const STATUS_LABEL: Record<string, string> = {
  capturing: "Capturing\u{2026}",
  uploading: "Saving to reel\u{2026}",
  saved: "Saved to reel \u{2713}",
  noauth: "Sign in to save photos",
};

function Keys({ keys }: { keys: string[] }) {
  return (
    <span className="cam__keys">
      {keys.map((k, i) =>
        k === "/" ? (
          <span key={i} className="cam__keysep">/</span>
        ) : (
          <kbd key={i} className={"cam__key" + (k.length > 2 ? " cam__key--wide" : "")}>{k}</kbd>
        )
      )}
    </span>
  );
}

type Coords = string | { x?: unknown; y?: unknown } | null | undefined;

function parseCoords(coords: Coords): [string, string] {
  if (!coords) return ["0", "0"];
  if (typeof coords === "string") {
    const [x, y] = coords.split(",").map((s) => s.trim());
    return [x || "0", y || "0"];
  }
  if (typeof coords === "object") return [String(coords.x ?? "0"), String(coords.y ?? "0")];
  return ["0", "0"];
}

function focusWorldCanvas() {
  if (typeof document === "undefined") return;
  const c = document.getElementById("mygame-canvas");
  const ae = document.activeElement;
  if (ae && (ae.tagName === "INPUT" || ae.tagName === "TEXTAREA" || (ae instanceof HTMLElement && ae.isContentEditable))) return;
  try {
    c?.focus({ preventScroll: true });
  } catch {
  }
}

type ReelImage = { id: string; url: string; thumbnailUrl?: string };

type CameraProps = { onClose?: () => void };

export default function Camera({ onClose }: CameraProps) {
  const identity = useBridgeState((s) => s.identity);
  const scene = useBridgeState((s) => s.scene);
  const [showShortcuts, setShowShortcuts] = useState(true);
  const [flash, setFlash] = useState(false);
  const [status, setStatus] = useState("idle");
  const [error, setError] = useState("");
  const [reel, setReel] = useState<ReelImage[]>([]);
  const [lightbox, setLightbox] = useState<string | null>(null);

  const address = identity?.address || null;

  const loadReel = useCallback(async () => {
    if (!address) return;
    try {
      const { status, body } = await signedFetch(
        `${serviceBase("cameraReel")}/api/users/${address}/images`,
        { method: "GET" }
      );
      if (status < 200 || status >= 300) return;
      const data = JSON.parse(body);
      setReel(Array.isArray(data?.images) ? data.images : []);
    } catch {
    }
  }, [address]);

  const upload = useCallback(
    async (dataUrl: string) => {
      if (!address) {
        setStatus("noauth");
        return;
      }
      const comma = dataUrl.indexOf(",");
      const image = comma >= 0 ? dataUrl.slice(comma + 1) : dataUrl;
      const [cx, cy] = parseCoords(scene?.coords);
      const body = {
        image,
        content_type: "image/png",
        metadata: {
          userName: identity?.name || "Guest",
          userAddress: address,
          dateTime: new Date().toISOString(),
          realm: scene?.realm || "",
          scene: { name: scene?.title || "", location: { x: cx, y: cy } },
          visiblePeople: [],
          placeId: "",
        },
        is_public: false,
      };
      setStatus("uploading");
      setError("");
      try {
        const { status: code, body: resBody } = await signedFetch(
          `${serviceBase("cameraReel")}/api/images-json`,
          { method: "POST", body, timeoutMs: 30000 }
        );
        if (code >= 200 && code < 300) {
          setStatus("saved");
          loadReel();
          setTimeout(() => setStatus((s) => (s === "saved" ? "idle" : s)), 2500);
        } else {
          let msg = `Upload failed (${code})`;
          try {
            msg = JSON.parse(resBody)?.message || msg;
          } catch {
          }
          setStatus("error");
          setError(msg);
        }
      } catch (e) {
        setStatus("error");
        setError(e instanceof Error ? e.message : "Upload failed");
      }
    },
    [address, identity, scene, loadReel]
  );

  useEffect(() => {
    const unsub = subscribeBridge((push: unknown) => {
      const p = push as { kind?: string; dataUrl?: string } | null;
      if (p && p.kind === "photo" && p.dataUrl) upload(p.dataUrl);
    });
    return unsub;
  }, [upload]);

  useEffect(() => {
    loadReel();
  }, [loadReel]);

  const takePhoto = useCallback(() => {
    if (!address) {
      setStatus("noauth");
      setFlash(true);
      setTimeout(() => setFlash(false), 220);
      return;
    }
    sendBridge("CapturePhoto");
    setStatus("capturing");
    setFlash(true);
    setTimeout(() => setFlash(false), 220);
    setTimeout(() => focusWorldCanvas(), 0);
  }, [address]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.code !== "Space" && e.key !== " ") return;
      const ae = document.activeElement;
      if (ae && (ae.tagName === "INPUT" || ae.tagName === "TEXTAREA" || (ae instanceof HTMLElement && ae.isContentEditable))) return;
      e.preventDefault();
      takePhoto();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [takePhoto]);

  useEffect(() => {
    document.body.classList.add("camera-open");
    sendBridge("SetCameraMode", { detached: true });
    const t = setTimeout(() => focusWorldCanvas(), 60);
    return () => {
      clearTimeout(t);
      sendBridge("SetCameraMode", { detached: false });
      document.body.classList.remove("camera-open");
    };
  }, []);

  return (
    <div className="cam">
      <div className="cam__view">
        <div className="cam__guides">
          <span className="cam__gv" style={{ left: "33.33%" }} />
          <span className="cam__gv" style={{ left: "66.66%" }} />
          <span className="cam__gh" style={{ top: "33.33%" }} />
          <span className="cam__gh" style={{ top: "66.66%" }} />
        </div>
        <span className="cam__crop cam__crop--tl" />
        <span className="cam__crop cam__crop--tr" />
        <span className="cam__crop cam__crop--bl" />
        <span className="cam__crop cam__crop--br" />
      </div>

      {showShortcuts && (
        <aside className="cam__shortcuts">
          <div className="cam__shorthead">
            <h3 className="cam__shorttitle">Camera Controls</h3>
            <button className="cam__shortclose" onClick={() => setShowShortcuts(false)} aria-label="Close">&#xD7;</button>
          </div>
          <div className="cam__shortlist">
            {SHORTCUTS.map((s) => (
              <div className="cam__shortrow" key={s.action}>
                <span className="cam__shortaction">{s.action}</span>
                <Keys keys={s.keys} />
              </div>
            ))}
          </div>
        </aside>
      )}

      {status !== "idle" && (
        <div className={"cam__status cam__status--" + status} role="status" aria-live="polite">
          {status === "error" ? error || "Upload failed" : STATUS_LABEL[status]}
        </div>
      )}

      {reel.length > 0 && (
        <div className="cam__reelstrip" aria-label="Recent photos">
          {reel.slice(0, 8).map((img) => (
            <img
              key={img.id}
              className="cam__reelthumb"
              src={img.thumbnailUrl || img.url}
              alt="Reel photo"
              loading="lazy"
              role="button"
              tabIndex={0}
              style={{ cursor: "pointer" }}
              onClick={() => setLightbox(img.url)}
            />
          ))}
        </div>
      )}

      <div className="cam__hud">
        <button className="cam__reel" title="Camera Reel" data-sb-linkto="Explorer/Pages/Reel">
          <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
            <rect x="3" y="6" width="18" height="13" rx="2.5" fill="none" stroke="currentColor" strokeWidth="1.8" />
            <circle cx="12" cy="12.5" r="3.4" fill="none" stroke="currentColor" strokeWidth="1.8" />
            <path d="M8 6l1.5-2h5L16 6" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinejoin="round" />
          </svg>
          <span>Camera Reel</span>
        </button>

        <div className="cam__shutterwrap">
          <button
            className="cam__shutter"
            aria-label="Take photo"
            onClick={takePhoto}
            disabled={status === "capturing" || status === "uploading"}
          ><span /></button>
          <span className="cam__spacebar">[ SPACE BAR ]</span>
        </div>

        <div className="cam__hudright">
          <button
            className={"cam__iconbtn" + (showShortcuts ? " is-active" : "")}
            onClick={() => setShowShortcuts((s) => !s)}
            title="Camera Controls"
          >?</button>
          <button className="cam__iconbtn" aria-label="Close camera" onClick={onClose}>&#xD7;</button>
        </div>
      </div>

      {flash && <div className="cam__flash" aria-hidden="true" />}
      <Lightbox src={lightbox} onClose={() => setLightbox(null)} />
    </div>
  );
}
