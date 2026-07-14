import { buildViewportUrl } from "../catalyst/creator-hub/scene-editor";

import type { FoundryScene } from "./types";

// Everything a Play page needs to point at the real client, and nothing it needs
// to pretend. The embed URL is built by the same function the live scene editor
// uses; reachability is a probe of the world's *own* current scene, not an
// assumption and not a stale hardcoded entity.

export interface SceneEmbed {
  url: string;
  reachable: boolean;
  probedAt: string;
  /** The exact URL the probe fetched to decide reachability — the scene content
   *  the realm itself currently points at — so the page reports what it measured
   *  rather than diagnosing a cause it never observed. */
  probedUrl: string;
  /** The HTTP status the probe saw, or null on a timeout/refused connection. */
  status: number | null;
}

const PLAY_URL = "https://catalyst.example.com/play";
// The realm the Play deep link and embed point at. These are real, public
// Decentraland Worlds; the client must fetch their scene content from the public
// worlds-content-server, which is where it lives. Our own catalyst/worlds mirror
// answers /about "healthy" but does NOT host the scene bytes
// (worlds.example.com/contents/<entity> 404s), so a realm pointed there loads an
// empty world (verified 2026-08-16). Env-overridable for a node that does mirror
// the content.
const WORLD_REALM_BASE =
  process.env.FOUNDRY_WORLD_REALM_BASE ??
  "https://worlds-content-server.decentraland.org/world";
const EDITOR_HREF = "/creator-hub/scene-editor?new=1&from=foundry";
const PROBE_TIMEOUT_MS = 5_000;
const PROBE_MEMO_MS = 10 * 60_000;

function realmFor(scene: FoundryScene): string | null {
  return scene.worldName ? `${WORLD_REALM_BASE}/${scene.worldName}` : null;
}

/** The plain deep link: no COOP/COEP needed, opens the client in its own tab. */
export function worldLink(scene: FoundryScene): string | null {
  const realm = realmFor(scene);
  if (!realm) return null;
  return buildViewportUrl({ playUrl: PLAY_URL, realm, position: "0,0" });
}

/**
 * Pointer-seeding the editor is genesis-only, so a deployed world has no
 * per-world editor link that would actually open that world's scene. Both kinds
 * of row therefore open a new scene, tagged with where the click came from —
 * inventing a per-world editor URL would be a link that lies.
 */
export function editorUrl(_scene: FoundryScene): string {
  return EDITOR_HREF;
}

/**
 * Pull the entity id and content base a realm's `/about` currently declares for
 * its scene. The realm is the source of truth for what is deployed right now, so
 * a redeploy is followed automatically and no entity is hardcoded here.
 */
function parseSceneUrn(urn: unknown): { entityId: string; baseUrl: string } | null {
  if (typeof urn !== "string") return null;
  const m = /^urn:decentraland:entity:([^?]+)\?.*baseUrl=(.+)$/.exec(urn);
  if (!m) return null;
  const entityId = m[1];
  const baseUrl = m[2].endsWith("/") ? m[2] : `${m[2]}/`;
  return { entityId, baseUrl };
}

type ProbeResult = { reachable: boolean; status: number | null; probedUrl: string };
type ProbeMemo = ProbeResult & { at: number };
const probes = new Map<string, ProbeMemo>();

/**
 * Reachable means the world's scene content is actually fetchable — a realm that
 * reports `/about` healthy but 404s the very scene it points at is not playable,
 * and an embed of it would be a lie. So the probe reads the realm's own current
 * scene entity from `/about` and confirms that entity resolves at the base URL
 * the realm itself declares. Host-correct and current-state by construction.
 */
async function probeRealm(realm: string): Promise<ProbeResult> {
  const memo = probes.get(realm);
  const now = Date.now();
  if (memo && now - memo.at < PROBE_MEMO_MS) {
    return { reachable: memo.reachable, status: memo.status, probedUrl: memo.probedUrl };
  }

  const aboutUrl = `${realm}/about`;
  let result: ProbeResult = { reachable: false, status: null, probedUrl: aboutUrl };
  try {
    const res = await fetch(aboutUrl, {
      redirect: "follow",
      signal: AbortSignal.timeout(PROBE_TIMEOUT_MS),
    });
    if (!res.ok) {
      result = { reachable: false, status: res.status, probedUrl: aboutUrl };
    } else {
      const about = (await res.json()) as {
        healthy?: unknown;
        configurations?: { scenesUrn?: unknown[] };
      };
      const scene = parseSceneUrn(about?.configurations?.scenesUrn?.[0]);
      if (about?.healthy !== true || !scene) {
        // A realm that answers but declares no fetchable scene is not playable;
        // report the /about we measured, not a content URL we never tried.
        result = { reachable: false, status: res.status, probedUrl: aboutUrl };
      } else {
        const contentUrl = `${scene.baseUrl}${scene.entityId}`;
        try {
          const head = await fetch(contentUrl, {
            method: "HEAD",
            redirect: "follow",
            signal: AbortSignal.timeout(PROBE_TIMEOUT_MS),
          });
          result = { reachable: head.ok, status: head.status, probedUrl: contentUrl };
        } catch {
          result = { reachable: false, status: null, probedUrl: contentUrl };
        }
      }
    }
  } catch {
    // A refused connection or a timeout leaves no status; to a visitor it is the
    // same fact as an unreachable world, so the page reports what it saw and does
    // not diagnose why.
    result = { reachable: false, status: null, probedUrl: aboutUrl };
  }

  probes.set(realm, { ...result, at: now });
  if (probes.size > 200) {
    for (const [key, value] of probes) {
      if (now - value.at >= PROBE_MEMO_MS) probes.delete(key);
    }
  }
  return result;
}

/**
 * `null` for a scene that lives only in this repository — there is no deployed
 * world to join, and an embed pointed at nothing is worse than no embed.
 */
export async function sceneEmbed(scene: FoundryScene): Promise<SceneEmbed | null> {
  const url = worldLink(scene);
  const realm = realmFor(scene);
  if (!url || !realm) return null;
  const { reachable, status, probedUrl } = await probeRealm(realm);
  return {
    url,
    reachable,
    probedAt: new Date().toISOString(),
    probedUrl,
    status,
  };
}
