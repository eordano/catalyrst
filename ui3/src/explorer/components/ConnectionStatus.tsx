import { useEffect, useState } from "react";

import type { BridgeConnection } from "../../overlay/bridge";
import { useBridgeState } from "../../overlay/bridge";
import { getJSON } from "../../data/catalyst/client";
import SidebarDesignIcon from "../frames/SidebarDesignIcon";
import ConnectionLifecycleRow from "./ConnectionLifecycleRow";
import { fpsTone } from "./FpsMeter";
import { useFps, type FpsStats } from "./useFps";
import {
  aboutMatchesRealm,
  currentSceneId,
  debugLines,
  debugText,
  overlayBuildId,
  parseLiveScenes,
  parseRealmAbout,
  realmAboutBase,
  type DebugInfo,
  type LiveScene,
  type RealmAbout,
} from "./connectionDebug";
import "./connectionstatus.css";

type StatusKind = "ok" | "info" | "warn";

type StatusRow = {
  title: string;
  subtitle: string;
  status: StatusKind;
  label: string;
};

type ConnectionStatusProps = {
  connection?: BridgeConnection | null;
  realm?: string | null;
  onClose?: () => void;
  fps?: FpsStats;
  about?: RealmAbout | null;
  sceneId?: string | null;
  at?: Date;
  userAgent?: string | null;
  overlayBuild?: string;
};

type ClientEnv = {
  userAgent: string | null;
  overlayBuild: string;
  at: Date;
};

const SCENE_HEALTH: Record<
  BridgeConnection["sceneHealth"],
  { status: StatusKind; label: string }
> = {
  ok: { status: "ok", label: "Healthy" },
  error: { status: "warn", label: "Errors" },
  loading: { status: "info", label: "Loading" },
};

const OVERLAY_BUILD = overlayBuildId(import.meta.url);
const LIVE_SCENES_TIMEOUT_MS = 1500;
const COPIED_MS = 1500;

function rowsFor(
  connection: BridgeConnection | null | undefined,
  realm: string | null | undefined,
): StatusRow[] {
  const scene = connection ? SCENE_HEALTH[connection.sceneHealth] : null;
  return [
    {
      title: "Scene",
      subtitle: "Scene running with or without errors",
      status: scene?.status ?? "info",
      label: scene?.label ?? "\u{2026}",
    },
    {
      title: "Scene Room",
      subtitle: "Comms room for this scene",
      status: connection?.sceneRoom ? "ok" : "info",
      label: connection == null ? "\u{2026}" : connection.sceneRoom ? "Connected" : "None",
    },
    {
      title: "Global Room",
      subtitle: "Comms connection to the realm",
      status: connection?.globalRoom ? "ok" : "warn",
      label:
        connection == null ? "\u{2026}" : connection.globalRoom ? "Connected" : "Disconnected",
    },
    {
      title: "Realm",
      subtitle: "Connected realm",
      status: realm ? "ok" : "info",
      label: realm || "\u{2026}",
    },
  ];
}

function captureEnv(): ClientEnv {
  return {
    userAgent: typeof navigator === "undefined" ? null : navigator.userAgent || null,
    overlayBuild: OVERLAY_BUILD,
    at: new Date(),
  };
}

function useClientEnv(): [ClientEnv | null, (env: ClientEnv) => void] {
  const [env, setEnv] = useState<ClientEnv | null>(null);
  useEffect(() => {
    setEnv(captureEnv());
  }, []);
  return [env, setEnv];
}

async function queryLiveScenes(): Promise<LiveScene[]> {
  const fn = typeof window === "undefined" ? undefined : window.engine_console_command;
  if (typeof fn !== "function") return [];
  const timeout = new Promise<string>((_, reject) =>
    setTimeout(() => reject(new Error("live_scenes timeout")), LIVE_SCENES_TIMEOUT_MS),
  );
  return parseLiveScenes(await Promise.race([fn("/live_scenes"), timeout]));
}

function useRealmAbout(
  override: RealmAbout | null | undefined,
  realm: string | null,
): RealmAbout | null {
  const [fetched, setFetched] = useState<RealmAbout | null>(null);
  useEffect(() => {
    if (override !== undefined) return;
    let cancelled = false;
    const base = realmAboutBase(realm);
    getJSON<unknown>("/about", base ? { base } : {})
      .then((raw) => {
        if (cancelled) return;
        const about = parseRealmAbout(raw);
        setFetched(aboutMatchesRealm(about, realm) ? about : null);
      })
      .catch(() => {
        if (!cancelled) setFetched(null);
      });
    return () => {
      cancelled = true;
    };
  }, [override, realm]);
  return override === undefined ? fetched : override;
}

function useSceneId(override: string | null | undefined, title: string | null): string | null {
  const [scenes, setScenes] = useState<LiveScene[]>([]);
  useEffect(() => {
    if (override !== undefined) return;
    let cancelled = false;
    queryLiveScenes()
      .then((list) => {
        if (!cancelled) setScenes(list);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [override, title]);
  return override === undefined ? currentSceneId(scenes, title) : override;
}

export default function ConnectionStatus({
  connection,
  realm,
  onClose,
  fps,
  about,
  sceneId,
  at,
  userAgent,
  overlayBuild,
}: ConnectionStatusProps) {
  const lifecycle = useBridgeState((s) => s.lifecycle);
  const rows = rowsFor(connection, realm);
  const liveFps = useFps(fps === undefined);
  const stats = fps ?? liveFps;
  const scene = useBridgeState((s) => s.scene);
  const playerPosition = useBridgeState((s) => s.playerPosition);
  const address = useBridgeState((s) => s.identity.address);
  const liveRealm = realm ?? scene.realm ?? playerPosition?.realm ?? null;
  const realmAbout = useRealmAbout(about, liveRealm);
  const resolvedSceneId = useSceneId(sceneId, scene.title);
  const [env, setEnv] = useClientEnv();
  const [copyState, setCopyState] = useState<"idle" | "copied" | "failed">("idle");
  const [tab, setTab] = useState<"performance" | "connection">("performance");

  const info: DebugInfo = {
    realm: liveRealm,
    parcel: playerPosition?.parcel ?? scene.coords ?? null,
    position: playerPosition?.position ?? null,
    heading: playerPosition?.heading ?? null,
    sceneTitle: scene.title,
    sceneCoords: scene.coords,
    sceneId: resolvedSceneId,
    connection: connection ?? null,
    about: realmAbout,
    address,
    overlayBuild: overlayBuild ?? env?.overlayBuild ?? null,
    userAgent: userAgent === undefined ? (env?.userAgent ?? null) : userAgent,
    fps: stats,
    at: at ?? env?.at ?? null,
  };
  const lines = debugLines(info).filter(([key]) =>
    !["Scene health", "Scene room", "Global room", "Browser", "Captured", "FPS"].includes(key),
  );

  const onCopy = () => {
    const fresh = at === undefined ? captureEnv() : null;
    if (fresh) setEnv(fresh);
    const text = debugText(fresh ? { ...info, at: fresh.at } : info);
    const clipboard = typeof navigator === "undefined" ? undefined : navigator.clipboard;
    if (!clipboard?.writeText) {
      setCopyState("failed");
      return;
    }
    clipboard
      .writeText(text)
      .then(() => setCopyState("copied"))
      .catch(() => setCopyState("failed"));
  };

  useEffect(() => {
    if (copyState === "idle") return;
    const t = setTimeout(() => setCopyState("idle"), COPIED_MS);
    return () => clearTimeout(t);
  }, [copyState]);

  const tone = fpsTone(stats.engine ?? stats.page);
  return (
    <div className="xcs__stage">
      <div className="xcs" role="dialog" aria-label="Connection status">
        <div className="xcs__header">
          <span className="xcs__title">Connection status</span>
          <button className="xcs__close" aria-label="Close" onClick={onClose}>
            &#xD7;
          </button>
        </div>
        <div className="xcs__tabs" role="tablist" aria-label="Diagnostics">
          {(["performance", "connection"] as const).map((id) => (
            <button key={id} id={`xcs-tab-${id}`} role="tab" aria-selected={tab === id}
              aria-controls={`xcs-panel-${id}`} tabIndex={tab === id ? 0 : -1}
              onClick={() => setTab(id)} onKeyDown={(e) => {
                if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(e.key)) return;
                e.preventDefault();
                const next = e.key === "Home" ? "performance" : e.key === "End" ? "connection" : id === "performance" ? "connection" : "performance";
                setTab(next);
                document.getElementById(`xcs-tab-${next}`)?.focus();
              }}>{id === "performance" ? "Performance" : "Connection"}</button>
          ))}
        </div>
        <div role="tabpanel" id={`xcs-panel-${tab}`} aria-labelledby={`xcs-tab-${tab}`}>
        {tab === "connection" && rows.filter((r) => r.title !== "Realm").map((r, i) => (
          <div className="xcs__row" key={i}>
            <div className="xcs__info">
              <div className="xcs__rowtitle">{r.title}</div>
              <div className="xcs__subtitle">{r.subtitle}</div>
            </div>
            <span className={"xcs__pill xcs__pill--" + r.status}>
              <span className="xcs__dot" /> {r.label}
            </span>
          </div>
        ))}
        {tab === "connection" && lifecycle?.connections.map((connection) => (
          <ConnectionLifecycleRow key={`${lifecycle.session}:${connection.id}`} connection={connection} session={lifecycle.session} />
        ))}
        {tab === "performance" && <>
        <dl className="xcs__dl">
          <div className="xcs__dlrow"><dt className="xcs__dt">Engine frame rate</dt><dd className="xcs__dd"><span className={"xcs__fps is-" + tone} data-testid="xcs-fps"><span className="xcs__fpsnum">{stats.engine ?? "\u2014"}</span><span className="xcs__fpsunit">fps</span></span></dd></div>
          <div className="xcs__dlrow"><dt className="xcs__dt">Page frame rate</dt><dd className="xcs__dd">{stats.page} fps</dd></div>
          <div className="xcs__dlrow"><dt className="xcs__dt">Page frame interval</dt><dd className="xcs__dd">{stats.ms} ms</dd></div>
        </dl>

        </>}
        {tab === "connection" && <dl className="xcs__dl">
          {lines.map(([k, v]) => (
            <div className="xcs__dlrow" key={k}>
              <dt className="xcs__dt">{k}</dt>
              <dd className="xcs__dd"><span>{v}</span><CopyValue label={k} value={v} /></dd>
            </div>
          ))}
        </dl>}
        </div>
        <div className="xcs__section">
          <span className="xcs__sectiontitle">DEBUG INFO</span>
          <button className="xcs__copy" type="button" onClick={onCopy}>
            {copyState === "copied"
              ? "Copied"
              : copyState === "failed"
                ? "Copy failed"
                : "Copy debug info"}
          </button>
        </div>
      </div>
    </div>
  );
}

function CopyValue({ label, value }: { label: string; value: string }) {
  const [state, setState] = useState("idle");
  useEffect(() => { if (state === "idle") return; const timer = setTimeout(() => setState("idle"), 2000); return () => clearTimeout(timer); }, [state]);
  return <button type="button" className="xcs__copy-value" aria-label={state === "copied" ? `${label} copied` : `Copy ${label}`} title={state === "failed" ? "Copy failed. Try again." : `Copy ${label}`} onClick={() => { if (!navigator.clipboard) { setState("failed"); return; } void navigator.clipboard.writeText(value).then(() => setState("copied"), () => setState("failed")); }}><SidebarDesignIcon name="copy" /></button>;
}
