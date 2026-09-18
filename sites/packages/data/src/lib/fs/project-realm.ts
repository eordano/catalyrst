import { hasTemplateComposite, templateAssetContents } from "./template-composites";

const PROJECT_REALM_CACHE = "ch-project-v1";
const PROJECT_REALM_BASE = "/_project";

const GAME_BUNDLE_URL = "/template-bundles/games.js";

export const PLAY_STATE_FILE = "one-play-state";

type TypedIpfsRef = { file: string; hash: string };

type TemplateRealm = {
  publicUrl: string;
  content: TypedIpfsRef[];
  metadata: Record<string, unknown>;
  about: unknown;
};

export type PopulateResult = {
  ok: boolean;
  reason?: string;
  sceneHash?: string;
  assets?: number;
  game?: string;
  templateAssets?: number;
};

export type PopulateOptions = {
  template?: string | null;
  assets?: Record<string, string> | null;
  signal?: AbortSignal;
};

function abortError(): Error {
  const error = new Error("Project realm preparation was cancelled");
  error.name = "AbortError";
  return error;
}

function throwIfAborted(signal?: AbortSignal): void {
  if (signal?.aborted) throw abortError();
}

const NON_ASSET_RE =
  /(^|\/)(node_modules|\.git|bin)\/|(^|\/)(main\.composite|main\.crdt|scene\.json|package\.json|package-lock\.json|tsconfig\.json|\.gitignore)$|\.(ts|tsx|md|log)$/i;

function isAssetPath(p: string): boolean {
  return !NON_ASSET_RE.test(p);
}

const MIME: Record<string, string> = {
  glb: "model/gltf-binary",
  gltf: "model/gltf+json",
  bin: "application/octet-stream",
  png: "image/png",
  jpg: "image/jpeg",
  jpeg: "image/jpeg",
  gif: "image/gif",
  webp: "image/webp",
  ktx2: "image/ktx2",
  ktx: "image/ktx",
  basis: "application/octet-stream",
  hdr: "image/vnd.radiance",
  mp3: "audio/mpeg",
  m4a: "audio/mp4",
  wav: "audio/wav",
  ogg: "audio/ogg",
  mp4: "video/mp4",
  json: "application/json",
};

function contentTypeFor(path: string): string {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  return MIME[ext] ?? "application/octet-stream";
}

async function tokenFor(buf: ArrayBuffer): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", buf);
  const hex = Array.from(new Uint8Array(digest))
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
  return `b64-${hex}`;
}

function realmBaseOf(publicUrl: string): string {
  return publicUrl.replace(/\/content\/?$/, "");
}

async function fetchTemplateRealm(signal?: AbortSignal): Promise<TemplateRealm | null> {
  try {
    throwIfAborted(signal);
    const aboutRes = await fetch(`${PROJECT_REALM_BASE}/about`, { cache: "no-store", signal });
    throwIfAborted(signal);
    if (!aboutRes.ok) return null;
    const about = (await aboutRes.json()) as { content?: { publicUrl?: string } };
    throwIfAborted(signal);
    const publicUrl = about.content?.publicUrl?.replace(/\/$/, "");
    if (!publicUrl) return null;
    const actRes = await fetch(`${publicUrl}/entities/active`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ pointers: ["0,0"] }),
      cache: "no-store",
      signal,
    });
    throwIfAborted(signal);
    if (!actRes.ok) return null;
    const arr = (await actRes.json()) as Array<{
      content?: TypedIpfsRef[];
      metadata?: Record<string, unknown>;
    }>;
    throwIfAborted(signal);
    const ent = Array.isArray(arr) ? arr[0] : undefined;
    if (!ent || !Array.isArray(ent.content)) return null;
    return { publicUrl, content: ent.content, metadata: ent.metadata ?? {}, about };
  } catch (error) {
    if (signal?.aborted) throw error;
    return null;
  }
}

function mergeMetadata(
  templateMeta: Record<string, unknown>,
  sceneJsonText?: string | null,
): Record<string, unknown> {
  const meta: Record<string, unknown> = { ...templateMeta };
  try {
    if (sceneJsonText) {
      const sj = JSON.parse(sceneJsonText) as { display?: { title?: string } };
      const title = sj.display?.title;
      if (title) {
        const display = { ...(meta.display as Record<string, unknown> | undefined) };
        display.title = title;
        meta.display = display;
      }
    }
  } catch {
  }
  return meta;
}

function templateFromSceneJson(sceneJsonText?: string | null): string | null {
  try {
    if (!sceneJsonText) return null;
    const sj = JSON.parse(sceneJsonText) as { tags?: unknown };
    const tag = Array.isArray(sj.tags) ? (sj.tags[0] as unknown) : null;
    return typeof tag === "string" && hasTemplateComposite(tag) ? tag : null;
  } catch {
    return null;
  }
}

async function fetchGameBundle(signal?: AbortSignal): Promise<ArrayBuffer | null> {
  try {
    throwIfAborted(signal);
    const res = await fetch(GAME_BUNDLE_URL, { cache: "no-store", signal });
    throwIfAborted(signal);
    if (!res.ok) return null;
    const bundle = await res.arrayBuffer();
    throwIfAborted(signal);
    return bundle;
  } catch (error) {
    if (signal?.aborted) throw error;
    return null;
  }
}

export async function clearProjectRealm(signal?: AbortSignal): Promise<void> {
  throwIfAborted(signal);
  if (typeof caches === "undefined") return;
  try {
    const cache = await caches.open(PROJECT_REALM_CACHE);
    throwIfAborted(signal);
    for (const key of await cache.keys()) {
      throwIfAborted(signal);
      await cache.delete(key);
      throwIfAborted(signal);
    }
  } catch (error) {
    if (signal?.aborted) throw error;
  }
}

export async function populateProjectRealm(
  files: Record<string, File> | null,
  sceneJsonText?: string | null,
  opts: PopulateOptions = {},
): Promise<PopulateResult> {
  const { signal } = opts;
  throwIfAborted(signal);
  if (typeof caches === "undefined" || typeof crypto === "undefined" || !crypto.subtle) {
    return { ok: false, reason: "no-cache-or-subtlecrypto" };
  }
  if (!files) return { ok: false, reason: "no-files" };

  const template = await fetchTemplateRealm(signal);
  throwIfAborted(signal);
  if (!template) return { ok: false, reason: "template-realm-unreachable" };

  const assetPaths = Object.keys(files).filter(isAssetPath);
  const assetRefs: TypedIpfsRef[] = [];
  const assetBytes: { token: string; buf: ArrayBuffer; ct: string }[] = [];
  for (const p of assetPaths) {
    const buf = await files[p].arrayBuffer();
    throwIfAborted(signal);
    const token = await tokenFor(buf);
    throwIfAborted(signal);
    assetRefs.push({ file: p, hash: token });
    assetBytes.push({ token, buf, ct: contentTypeFor(p) });
  }

  const gameTemplate =
    (opts.template && hasTemplateComposite(opts.template) ? opts.template : null) ??
    templateFromSceneJson(sceneJsonText);
  let game: { token: string; buf: ArrayBuffer } | null = null;
  if (gameTemplate) {
    const buf = await fetchGameBundle(signal);
    throwIfAborted(signal);
    if (buf) {
      const token = await tokenFor(buf);
      throwIfAborted(signal);
      game = { token, buf };
    }
  }

  const templateAssets: { path: string; cid: string; buf: ArrayBuffer }[] = [];
  const taken = new Set(assetPaths.map((p) => p.toLowerCase()));
  const seedFromBuilder = async (path: string, cid: string) => {
    if (taken.has(path.toLowerCase())) return;
    try {
      throwIfAborted(signal);
      const res = await fetch(`/builder-items/${cid}`, { credentials: "omit", signal });
      throwIfAborted(signal);
      if (!res.ok) return;
      const buf = await res.arrayBuffer();
      throwIfAborted(signal);
      templateAssets.push({ path, cid, buf });
      taken.add(path.toLowerCase());
    } catch (error) {
      if (signal?.aborted) throw error;
    }
  };
  if (gameTemplate) {
    for (const [path, cid] of Object.entries(templateAssetContents(gameTemplate))) {
      await seedFromBuilder(path, cid);
    }
  }
  for (const [path, cid] of Object.entries(opts.assets ?? {})) {
    if (typeof cid === "string" && cid) await seedFromBuilder(path, cid);
  }

  let content: TypedIpfsRef[] = [
    ...template.content,
    ...assetRefs,
    ...templateAssets.map((a) => ({ file: a.path, hash: a.cid })),
  ];
  if (game !== null) {
    const gameRef = { file: "bin/index.js", hash: game.token };
    content = content.some((c) => c.file === "bin/index.js")
      ? content.map((c) => (c.file === "bin/index.js" ? gameRef : c))
      : [...content, gameRef];
  }

  const metadata = mergeMetadata(template.metadata, sceneJsonText);
  if (game !== null && gameTemplate) {
    metadata.one_play = { template: gameTemplate, gated: true };
  }
  const runtimeBytes: { token: string; buf: ArrayBuffer; ct: string }[] = [];
  for (const ref of content) {
    if (ref.file !== "main.crdt" && (game !== null || ref.file !== metadata.main)) continue;
    const response = await fetch(`${template.publicUrl}/contents/${encodeURIComponent(ref.hash)}`, {
      cache: "no-store", signal,
    });
    throwIfAborted(signal);
    if (!response.ok) return { ok: false, reason: "template-content-unreachable" };
    const buf = await response.arrayBuffer();
    throwIfAborted(signal);
    runtimeBytes.push({ token: ref.hash, buf, ct: ref.file.endsWith(".js") ? "application/javascript" : "application/octet-stream" });
  }
  const sceneHash = await tokenFor(
    new TextEncoder().encode(JSON.stringify(content) + "|0,0").buffer as ArrayBuffer,
  );
  throwIfAborted(signal);

  const entity = {
    id: sceneHash,
    version: "v3",
    type: "scene",
    pointers: ["0,0"],
    timestamp: Date.now(),
    content,
    metadata,
  };

  const about =
    template.about && typeof template.about === "object"
      ? (JSON.parse(JSON.stringify(template.about)) as Record<string, unknown>)
      : {};
  const cfg =
    about.configurations && typeof about.configurations === "object"
      ? (about.configurations as Record<string, unknown>)
      : (about.configurations = {} as Record<string, unknown>);
  cfg.cityLoaderContentServer = template.publicUrl;
  cfg.scenesUrn = [
    `urn:decentraland:entity:${sceneHash}?=&baseUrl=${template.publicUrl}/contents/`,
  ];

  const cache = await caches.open(PROJECT_REALM_CACHE);
  throwIfAborted(signal);
  for (const key of await cache.keys()) {
    throwIfAborted(signal);
    await cache.delete(key);
    throwIfAborted(signal);
  }

  const json = (v: unknown) => JSON.stringify(v);
  const headers = (ct: string) => ({
    "content-type": ct,
    "access-control-allow-origin": "*",
  });
  const put = async (url: string, body: BodyInit, ct: string) => {
    throwIfAborted(signal);
    await cache.put(url, new Response(body, { headers: headers(ct) }));
    throwIfAborted(signal);
  };

  await put(`${realmBaseOf(template.publicUrl)}/about`, json(about), "application/json");
  await put(`${template.publicUrl}/entities/active`, json([entity]), "application/json");
  await put(`${template.publicUrl}/contents/${sceneHash}`, json(entity), "application/json");
  for (const a of [...assetBytes, ...runtimeBytes]) {
    await put(`${template.publicUrl}/contents/${a.token}`, a.buf, a.ct);
  }
  for (const a of templateAssets) {
    await put(`${template.publicUrl}/contents/${a.cid}`, a.buf, contentTypeFor(a.path));
  }
  if (game !== null) {
    await put(`${template.publicUrl}/contents/${game.token}`, game.buf, "application/javascript");
    await put(
      `${template.publicUrl}/contents/${PLAY_STATE_FILE}`,
      json({ playing: false }),
      "application/json",
    );
  }

  return {
    ok: true,
    sceneHash,
    assets: assetBytes.length,
    templateAssets: templateAssets.length,
    ...(game !== null && gameTemplate ? { game: gameTemplate } : {}),
  };
}

export async function populateTemplateRealm(opts: {
  template: string;
  name?: string;
  assets?: Record<string, string> | null;
  signal?: AbortSignal;
}): Promise<PopulateResult> {
  throwIfAborted(opts.signal);
  if (!hasTemplateComposite(opts.template)) {
    return { ok: false, reason: "unknown-template" };
  }
  const sceneJsonText = opts.name
    ? JSON.stringify({ display: { title: opts.name }, tags: [opts.template] })
    : JSON.stringify({ tags: [opts.template] });
  return populateProjectRealm({}, sceneJsonText, {
    template: opts.template,
    assets: opts.assets ?? null,
    signal: opts.signal,
  });
}
