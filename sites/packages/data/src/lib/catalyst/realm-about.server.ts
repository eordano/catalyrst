import { getJSON, type GetOptions } from "./client";
import { ttlMemo } from "../ttl-memo";

const REALM_ABOUT_TTL_MS = 60_000;

const aboutMemo = ttlMemo({
  ttlMs: REALM_ABOUT_TTL_MS,
  keyOf: (opts: GetOptions) => (opts.fetchImpl || opts.base ? null : ""),
  keep: (raw) => raw !== null && typeof raw === "object" && !Array.isArray(raw),
  load: (opts) => getJSON<unknown>("/about", opts.fetchImpl || opts.base ? opts : {}),
});

// The realm's /about is visitor-independent; one read per minute serves every overlay widget.
export function loadRealmAbout(opts: GetOptions = {}): Promise<unknown> {
  return aboutMemo(opts);
}

export function resetRealmAboutCache(): void {
  aboutMemo.reset();
}
