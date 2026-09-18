import type { BridgeConnection } from "../../overlay/bridge";
import type { FpsStats } from "./useFps";

export type RealmAbout = {
  realmName: string | null;
  commsProtocol: string | null;
  commsAdapter: string | null;
  commsVersion: string | null;
  contentVersion: string | null;
  lambdasVersion: string | null;
  usersCount: number | null;
};

export const EMPTY_ABOUT: RealmAbout = {
  realmName: null,
  commsProtocol: null,
  commsAdapter: null,
  commsVersion: null,
  contentVersion: null,
  lambdasVersion: null,
  usersCount: null,
};

function rec(v: unknown): Record<string, unknown> | null {
  return v !== null && typeof v === "object" ? (v as Record<string, unknown>) : null;
}

function str(v: unknown): string | null {
  return typeof v === "string" && v !== "" ? v : null;
}

function num(v: unknown): number | null {
  return typeof v === "number" && Number.isFinite(v) ? v : null;
}

export function parseRealmAbout(raw: unknown): RealmAbout {
  const root = rec(raw);
  if (!root) return EMPTY_ABOUT;
  const conf = rec(root.configurations);
  const comms = rec(root.comms);
  const content = rec(root.content);
  const lambdas = rec(root.lambdas);
  return {
    realmName: str(conf?.realmName),
    commsProtocol: str(comms?.protocol),
    commsAdapter: str(comms?.adapter) ?? str(comms?.fixedAdapter),
    commsVersion: str(comms?.version),
    contentVersion: str(content?.version),
    lambdasVersion: str(lambdas?.version),
    usersCount: num(comms?.usersCount),
  };
}

export function realmAboutBase(realm: string | null | undefined): string | null {
  if (!realm) return null;
  let url: URL;
  try {
    url = new URL(realm);
  } catch {
    return null;
  }
  if (url.protocol !== "http:" && url.protocol !== "https:") return null;
  const path = url.pathname.replace(/\/about\/?$/, "").replace(/\/+$/, "");
  return `${url.origin}${path}`;
}

export function aboutMatchesRealm(about: RealmAbout, realm: string | null | undefined): boolean {
  if (!realm || realmAboutBase(realm)) return true;
  return about.realmName === null || about.realmName === realm;
}

export function overlayBuildId(moduleUrl: string): string {
  const file = moduleUrl.split(/[?#]/)[0]?.split("/").pop() ?? "";
  return file.replace(/\.[cm]?[jt]sx?$/, "") || "dev";
}

export type LiveScene = {
  title: string;
  hash: string;
  portable: boolean;
  broken: boolean;
};

const LIVE_SCENE_LINE = /^(.*) \[([^\]]+)\]$/;

export function parseLiveScenes(text: string): LiveScene[] {
  return text
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean)
    .flatMap((line) => {
      const m = line.match(LIVE_SCENE_LINE);
      if (!m) return [];
      const [hash = "", ...flags] = (m[2] ?? "").split(",").map((s) => s.trim());
      return [
        {
          title: m[1] ?? "",
          hash,
          portable: flags.includes("portable"),
          broken: flags.includes("broken"),
        },
      ];
    });
}

export function currentSceneId(scenes: LiveScene[], title: string | null): string | null {
  const worlds = scenes.filter((s) => !s.portable);
  const byTitle = title ? worlds.find((s) => s.title === title) : undefined;
  const pick = byTitle ?? (worlds.length === 1 ? worlds[0] : undefined);
  return pick?.hash || null;
}

export type DebugInfo = {
  realm: string | null;
  parcel: string | null;
  position: [number, number, number] | null;
  heading: number | null;
  sceneTitle: string | null;
  sceneCoords: string | null;
  sceneId: string | null;
  connection: BridgeConnection | null;
  about: RealmAbout | null;
  address: string | null;
  overlayBuild: string | null;
  userAgent: string | null;
  fps: FpsStats | null;
  at: Date | null;
};

export function formatPosition(p: [number, number, number] | null | undefined): string | null {
  if (!p) return null;
  return p.map((n) => (Number.isFinite(n) ? n.toFixed(1) : "?")).join(", ");
}

function roomState(connected: boolean | undefined): string {
  if (connected === undefined) return "unknown";
  return connected ? "connected" : "none";
}

export function debugLines(d: DebugInfo): [string, string][] {
  const out: [string, string][] = [];
  const push = (k: string, v: string | null | undefined) => {
    if (v) out.push([k, v]);
  };
  const about = d.about;
  push("Realm", d.realm ?? about?.realmName);
  push("Parcel", d.parcel);
  const pos = formatPosition(d.position);
  push(
    "Position",
    pos ? (d.heading === null ? pos : `${pos} \u00b7 heading ${Math.round(d.heading)}\u00b0`) : null,
  );
  push(
    "Scene",
    d.sceneTitle
      ? d.sceneCoords
        ? `${d.sceneTitle} (${d.sceneCoords})`
        : d.sceneTitle
      : d.sceneCoords,
  );
  push("Scene id", d.sceneId);
  const c = d.connection;
  push("Scene health", c?.sceneHealth);
  push("Scene room", c ? roomState(c.sceneRoom) : null);
  push("Global room", c ? roomState(c.globalRoom) : null);
  push(
    "Comms",
    about?.commsProtocol || about?.commsAdapter
      ? [about.commsProtocol, about.commsAdapter].filter(Boolean).join(" \u00b7 ")
      : null,
  );
  push("Users online", about?.usersCount === null || !about ? null : String(about.usersCount));
  push(
    "Server",
    about &&
      (about.contentVersion || about.lambdasVersion || about.commsVersion)
      ? [
          about.contentVersion ? `content ${about.contentVersion}` : null,
          about.lambdasVersion ? `lambdas ${about.lambdasVersion}` : null,
          about.commsVersion ? `comms ${about.commsVersion}` : null,
        ]
          .filter(Boolean)
          .join(" \u00b7 ")
      : null,
  );
  push("Overlay build", d.overlayBuild);
  push("Address", d.address);
  push(
    "FPS",
    d.fps
      ? `page ${d.fps.page} \u00b7 engine ${d.fps.engine ?? "n/a"} \u00b7 ${d.fps.ms} ms/frame`
      : null,
  );
  push("Browser", d.userAgent);
  push("Captured", d.at ? d.at.toISOString() : null);
  return out;
}

export function debugText(d: DebugInfo): string {
  return debugLines(d)
    .map(([k, v]) => `${k}: ${v}`)
    .join("\n");
}
