import { catalystBase, getJSON, sendJSON, type RequestOpts } from "./client";
import { field, isRecord, listOf } from "./rows";

const ADDRESS_RE = /^0x[0-9a-f]{40}$/i;

export function asAddress(raw: unknown): string | null {
  if (typeof raw !== "string") return null;
  const s = raw.trim();
  return ADDRESS_RE.test(s) ? s.toLowerCase() : null;
}

type SceneEntityRef = { id: string; title: string | null };

export function sceneEntityForParcel(entities: unknown, parcel: string): SceneEntityRef | null {
  const want = parcel.trim().toLowerCase();
  for (const e of listOf<unknown>(entities)) {
    if (!isRecord(e) || typeof e.id !== "string" || !e.id) continue;
    const pointers = listOf<unknown>(e.pointers).map((p) => String(p).trim().toLowerCase());
    if (!pointers.includes(want)) continue;
    const title = field(field(field(e, "metadata"), "display"), "title");
    return { id: e.id, title: typeof title === "string" && title.trim() ? title.trim() : null };
  }
  return null;
}

export function deployerFromAuthChain(audit: unknown): string | null {
  for (const link of listOf<unknown>(field(audit, "authChain"))) {
    if (isRecord(link) && link.type === "SIGNER") return asAddress(link.payload);
  }
  return null;
}

type SceneDeployment = { entityId: string; title: string | null; deployer: string | null };

export async function fetchSceneDeployment(
  parcel: string,
  opts: RequestOpts = {},
): Promise<SceneDeployment | null> {
  const entities = await sendJSON("/content/entities/active", {
    ...opts,
    method: "POST",
    body: { pointers: [parcel] },
  });
  const entity = sceneEntityForParcel(entities, parcel);
  if (!entity) return null;
  const audit = await getJSON(`/content/audit/scene/${encodeURIComponent(entity.id)}`, opts);
  return { entityId: entity.id, title: entity.title, deployer: deployerFromAuthChain(audit) };
}

type HomeRealm = { name: string | null; base: string };

export function homeRealmFromAbout(about: unknown, base: string): HomeRealm {
  const name = field(field(about, "configurations"), "realmName");
  return { name: typeof name === "string" && name.trim() ? name.trim() : null, base };
}

export async function fetchHomeRealm(opts: RequestOpts = {}): Promise<HomeRealm> {
  const base = catalystBase();
  const about = await getJSON("/about", { ...opts, base });
  return homeRealmFromAbout(about, base);
}

export function isHomeRealm(
  realm: string | null | undefined,
  home: HomeRealm | null | undefined,
): boolean {
  if (!realm || !home) return false;
  const r = realm.trim().replace(/\/+$/, "").toLowerCase();
  if (!r) return false;
  if (home.name && r === home.name.toLowerCase()) return true;
  const base = home.base.trim().replace(/\/+$/, "").toLowerCase();
  return base !== "" && (r === base || r === `${base}/about`);
}

export const SCENE_OWNER_STALE_MS = 5 * 60 * 1000;

export const sceneOwnerKeys = {
  homeRealm: () => ["realm-about"] as const,
  deployment: (parcel: string | null) => ["scene-deployment", parcel] as const,
};
