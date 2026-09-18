import { catalystBase, getJSON, sendJSON, type RequestOpts } from "./client";
import { field, isRecord, listOf } from "./rows";

const ADDRESS_RE = /^0x[0-9a-f]{40}$/i;

export function asAddress(raw: unknown): string | null {
  if (typeof raw !== "string") return null;
  const s = raw.trim();
  return ADDRESS_RE.test(s) ? s.toLowerCase() : null;
}

type SceneEntityRef = { id: string; title: string | null; tipAddress?: string; sceneAuthor?: string };

function metadataAddresses(metadata: unknown) {
  const address = (key: string) => asAddress(field(metadata, key)) ?? asAddress(field(field(metadata, key), "address"));
  const tipAddress = address("tipAddress");
  const sceneAuthor = address("sceneAuthor");
  return { ...(tipAddress ? { tipAddress } : {}), ...(sceneAuthor ? { sceneAuthor } : {}) };
}

export function sceneEntityForParcel(entities: unknown, parcel: string): SceneEntityRef | null {
  const want = parcel.trim().toLowerCase();
  for (const e of listOf<unknown>(entities)) {
    if (!isRecord(e) || typeof e.id !== "string" || !e.id) continue;
    const pointers = listOf<unknown>(e.pointers).map((p) => String(p).trim().toLowerCase());
    if (!pointers.includes(want)) continue;
    const title = field(field(field(e, "metadata"), "display"), "title");
    return { id: e.id, title: typeof title === "string" && title.trim() ? title.trim() : null, ...metadataAddresses(e.metadata) };
  }
  return null;
}

export function deployerFromAuthChain(audit: unknown): string | null {
  for (const link of listOf<unknown>(field(audit, "authChain"))) {
    if (isRecord(link) && link.type === "SIGNER") return asAddress(link.payload);
  }
  return null;
}

export type SceneDeployment = { entityId: string; title: string | null; deployer: string | null; tipAddress?: string; sceneAuthor?: string };

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
  const audit = await getJSON(`/content/audit/scene/${encodeURIComponent(entity.id)}`, opts).catch(error => {
    if (opts.signal?.aborted) throw error;
    return null;
  });
  const { id, ...metadata } = entity;
  return { entityId: id, ...metadata, deployer: deployerFromAuthChain(audit) };
}

export async function fetchLandOwner(parcel: string, opts: RequestOpts = {}): Promise<string | null> {
  const [x, y] = parcel.split(",");
  const rights = await getJSON(`/lambdas/parcels/${encodeURIComponent(x!)}/${encodeURIComponent(y!)}/operators`, opts);
  return asAddress(field(rights, "owner"));
}

export async function fetchWorldSceneDeployment(realm: string, base: string, parcel: string, opts: RequestOpts = {}): Promise<SceneDeployment | null> {
  const about = await getJSON("/about", { ...opts, base });
  const config = field(about, "configurations");
  if (realm !== base && realm !== `${base}/about` && field(config, "realmName") !== realm) return null;
  const publicUrl = field(field(about, "content"), "publicUrl");
  const contentBase = typeof publicUrl === "string" && /^https?:\/\//.test(publicUrl) ? publicUrl.replace(/\/$/, "") : `${base}/content`;
  const scenes = await Promise.all(listOf<unknown>(field(config, "scenesUrn")).map(async raw => {
    if (typeof raw !== "string" || !raw.startsWith("urn:decentraland:entity:")) return null;
    const urn = new URL(raw);
    const id = urn.pathname.split(":").at(-1)!;
    const baseUrl = urn.searchParams.get("baseUrl");
    const contents = baseUrl && /^https?:\/\//.test(baseUrl) ? baseUrl.replace(/\/$/, "") : `${contentBase}/contents`;
    const entity = await getJSON(`/${encodeURIComponent(id)}`, { ...opts, base: contents });
    return isRecord(entity) ? { ...entity, id } : null;
  }));
  const entity = sceneEntityForParcel(scenes, parcel);
  if (!entity) return null;
  const audit = await getJSON(`/audit/scene/${encodeURIComponent(entity.id)}`, { ...opts, base: contentBase }).catch(error => {
    if (opts.signal?.aborted) throw error;
    return null;
  });
  const { id, ...metadata } = entity;
  return { entityId: id, ...metadata, deployer: deployerFromAuthChain(audit) };
}

export type SceneRecipientRole = "tipAddress" | "sceneAuthor" | "sceneDeployer" | "landOwner";
export type SceneRecipient = { role: SceneRecipientRole; label: string; source: string; address: string | null; unavailable: string };

export function sceneRecipients(deployment: SceneDeployment | null | undefined, landOwner: string | null | undefined, world = false): SceneRecipient[] {
  return [
    { role: "tipAddress", label: "Tip address", source: "scene.json \u00b7 tipAddress", address: deployment?.tipAddress ?? null, unavailable: "No valid tip address provided" },
    { role: "sceneAuthor", label: "Scene author", source: "scene.json \u00b7 sceneAuthor", address: deployment?.sceneAuthor ?? null, unavailable: "No valid author address provided" },
    { role: "sceneDeployer", label: "Scene deployer", source: "Scene deployment \u00b7 signing wallet", address: deployment?.deployer ?? null, unavailable: "Deployment signer unavailable" },
    { role: "landOwner", label: "Land owner", source: "Ethereum LAND / estate ownership \u00b7 blockchain index", address: world ? null : landOwner ?? null, unavailable: world ? "Not applicable in Worlds" : "Land ownership unavailable" },
  ];
}

type HomeRealm = { name: string | null; base: string; publicBase?: string };

export function homeRealmFromAbout(about: unknown, base: string): HomeRealm {
  const name = field(field(about, "configurations"), "realmName");
  const content = field(field(about, "content"), "publicUrl");
  const publicBase = typeof content === "string" && /\/content\/?$/.test(content) ? content.replace(/\/content\/?$/, "") : undefined;
  return { name: typeof name === "string" && name.trim() ? name.trim() : null, base, ...(publicBase ? { publicBase } : {}) };
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
  return [home.base, home.publicBase].some(value => {
    const base = value?.trim().replace(/\/+$/, "").toLowerCase();
    return !!base && (r === base || r === `${base}/about`);
  });
}

export const SCENE_OWNER_STALE_MS = 5 * 60 * 1000;

export function worldRealmBase(realm: string, launchRealm: string | null): string | null {
  for (const candidate of [realm, launchRealm]) {
    if (!candidate) continue;
    try {
      const url = new URL(candidate);
      if (!/^https?:$/.test(url.protocol) || url.username || url.password) continue;
      const path = url.pathname.replace(/\/about\/?$/, "").replace(/\/+$/, "");
      if (/\/world\/[^/]+$/.test(path)) return `${url.origin}${path}`;
    } catch {}
  }
  return null;
}

export async function fetchWorldOwner(realm: string, base: string, opts: RequestOpts = {}) {
  const about = await getJSON("/about", { ...opts, base });
  const name = field(field(about, "configurations"), "realmName");
  if (realm !== base && realm !== `${base}/about` && name !== realm) return null;
  const permissions = await getJSON("/permissions", { ...opts, base });
  return { deployer: asAddress(field(permissions, "owner")), title: typeof name === "string" ? name : null };
}

export const sceneOwnerKeys = {
  homeRealm: () => ["realm-about"] as const,
  deployment: (parcel: string | null) => ["scene-deployment", parcel] as const,
  world: (realm: string | null | undefined, base: string | null) => ["world-owner", realm, base] as const,
};
