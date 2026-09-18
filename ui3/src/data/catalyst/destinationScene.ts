import { catalystBase, getJSON, sendJSON, type RequestOpts } from "./client";
import { fetchPlaces, fetchWorlds } from "./placesSchema";
import { field, listOf } from "./rows";

export type SceneDestination = { realm?: string; coords?: string };
export type DestinationScene = { title: string; image?: string | null; creator?: string | null };

export function destinationLabel(destination: SceneDestination): string {
  if (!destination.realm) return `Parcel ${destination.coords || "0,0"}`;
  try {
    const url = new URL(destination.realm);
    return decodeURIComponent(url.pathname.replace(/\/about\/?$/, "").split("/").filter(Boolean).at(-1) || url.hostname);
  } catch { return destination.realm; }
}

function sceneDetails(entity: unknown, contentBase: string): DestinationScene | null {
  const metadata = field(entity, "metadata");
  const display = field(metadata, "display");
  const title = field(display, "title");
  if (typeof title !== "string" || !title.trim()) return null;
  const thumbnail = field(display, "navmapThumbnail");
  const content = listOf<unknown>(field(entity, "content"));
  const hash = field(content.find(file => field(file, "file") === thumbnail), "hash");
  const image = typeof hash === "string" ? `${contentBase.replace(/\/$/, "")}/contents/${encodeURIComponent(hash)}`
    : typeof thumbnail === "string" && /^https?:\/\//.test(thumbnail) ? thumbnail : null;
  const author = field(field(metadata, "contact"), "name");
  return { title: title.trim(), image, creator: typeof author === "string" ? author : null };
}

export async function fetchDestinationScene(destination: SceneDestination, opts: RequestOpts = {}): Promise<DestinationScene | null> {
  const name = destinationLabel(destination);
  const indexed = destination.realm
    ? await fetchWorlds({ names: name, limit: 1 }, opts)
    : await fetchPlaces({ positions: destination.coords, limit: 1 }, opts);
  if (indexed[0]) return indexed[0];
  let contentBase = `${catalystBase(opts.base)}/content`;
  if (destination.realm) {
    const base = /^https?:\/\//.test(destination.realm)
      ? destination.realm.replace(/\/about\/?$/, "").replace(/\/$/, "")
      : `${catalystBase(opts.base)}/world/${encodeURIComponent(destination.realm)}`;
    const about = await getJSON("/about", { ...opts, base });
    const publicUrl = field(field(about, "content"), "publicUrl");
    if (typeof publicUrl === "string" && /^https?:\/\//.test(publicUrl)) contentBase = publicUrl.replace(/\/$/, "");
    const urns = listOf<unknown>(field(field(about, "configurations"), "scenesUrn"));
    if (!destination.coords && typeof urns[0] === "string") {
      const urn = new URL(urns[0]);
      const hash = urn.pathname.split(":").at(-1);
      const baseUrl = urn.searchParams.get("baseUrl");
      const contents = baseUrl && /^https?:\/\//.test(baseUrl) ? baseUrl.replace(/\/$/, "") : `${contentBase}/contents`;
      if (hash) return sceneDetails(await getJSON(`/${encodeURIComponent(hash)}`, { ...opts, base: contents }), contents.replace(/\/contents$/, ""));
    }
  }
  const entities = await sendJSON("/entities/active", { ...opts, base: contentBase, method: "POST", body: { pointers: [destination.coords || "0,0"] } });
  const entity = listOf<unknown>(entities).find(item => listOf(field(item, "pointers")).includes(destination.coords || "0,0"));
  return sceneDetails(entity, contentBase);
}
