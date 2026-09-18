import { publicImageUrl } from "../publicImage";
import { normalizeEvent } from "../catalyst/events";
import type { PlayScreen, PlaySectionMessage, PlaySectionName } from "./play";

const names: PlaySectionName[] = ["featured", "places", "events", "wearables", "emotes", "outfits"];
const supported: PlaySectionName[] = [...names, "upcoming"];

function validate(message: PlaySectionMessage, address: string) {
  if (message?.version !== 1 || message.address !== address || !supported.includes(message.section)) {
    throw new Error("Invalid play screen response");
  }
  const result = message.result;
  if (result?.status === "unavailable" && result.data === null) return;
  if (result?.status !== "ready" || !Number.isFinite(result.updatedAt)) throw new Error("Invalid play section");
  const data = result.data;
  if (message.section === "featured" || message.section === "places" || message.section === "outfits") {
    if (Array.isArray(data)) return;
  } else if (data && typeof data === "object") {
    if ((message.section === "events" || message.section === "upcoming") && "data" in data && Array.isArray(data.data) && "total" in data && typeof data.total === "number") return;
    if ("catalog" in data && Array.isArray(data.catalog) && "owned" in data && Array.isArray(data.owned)) {
      if (message.section === "wearables" && "equipped" in data && data.equipped) return;
      if (message.section === "emotes" && "loadout" in data && Array.isArray(data.loadout)) return;
    }
  }
  throw new Error("Invalid play section data");
}

function normalizeImages(message: PlaySectionMessage) {
  if (message.result.status !== "ready") return;
  if (message.section === "featured" || message.section === "places") {
    message.result.data = message.result.data.map(place => ({ ...place, image: publicImageUrl(place.image) }));
  } else if (message.section === "events" || message.section === "upcoming") {
    message.result.data = { ...message.result.data, data: message.result.data.data.map(normalizeEvent) };
  }
}

export async function readPlayStream(
  response: Response,
  address: string,
  onSection: (message: PlaySectionMessage) => void,
): Promise<PlayScreen> {
  if (!response.ok) throw new Error(`Play data returned ${response.status}`);
  if (!response.headers.get("content-type")?.includes("application/x-ndjson")) {
    const data = await response.json() as PlayScreen;
    for (const section of supported) {
      if (section === "upcoming" && data.sections?.upcoming === undefined) continue;
      const message = { version: data.version, address: data.address, section, result: data.sections?.[section] } as PlaySectionMessage;
      validate(message, address);
      normalizeImages(message);
      onSection(message);
    }
    return data;
  }
  if (!response.body) throw new Error("Missing play screen stream");
  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  const sections: Partial<PlayScreen["sections"]> = {};
  let pending = "";
  let completed = false;
  let size = 0;
  const line = (raw: string) => {
    if (!raw.trim()) return;
    if (completed) throw new Error("Unexpected play data after completion");
    const message = JSON.parse(raw);
    if (message.done === true) {
      if (message.version !== 1 || message.address !== address || names.some((name) => !sections[name])) {
        throw new Error("Incomplete play screen");
      }
      completed = true;
      return;
    }
    validate(message, address);
    normalizeImages(message);
    const { section, result } = message as PlaySectionMessage;
    if (sections[section]) throw new Error("Duplicate play section");
    Object.assign(sections, { [section]: result });
    onSection(message);
  };
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      size += value.byteLength;
      if (size > 16 * 1024 * 1024) throw new Error("Play screen exceeds the payload limit");
      pending += decoder.decode(value, { stream: true });
      let newline: number;
      while ((newline = pending.indexOf("\n")) !== -1) {
        line(pending.slice(0, newline));
        pending = pending.slice(newline + 1);
      }
    }
    pending += decoder.decode();
    if (pending.trim()) line(pending);
    if (!completed) throw new Error("Incomplete play screen");
    return { version: 1, address, sections: sections as PlayScreen["sections"] };
  } finally {
    await reader.cancel().catch(() => {});
    reader.releaseLock();
  }
}
