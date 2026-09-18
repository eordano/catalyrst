import { catalystBase } from "./catalyst/client";

export function publicThumbnail(source: string | null | undefined, width: 320 | 640 | 960 = 640): string | undefined {
  if (!source) return undefined;
  try {
    const url = new URL(source);
    if (url.protocol !== "https:" || url.search || /\.(gif|svg)$/i.test(url.pathname)) return source;
    const known = /^(peer(?:-[a-z]+\d)?|worlds-content-server|events-assets-099ac00|assets-cdn)\.decentraland\.org$/.test(url.hostname);
    if (!known) return source;
    return `${catalystBase()}/media/convert?width=${width}&url=${encodeURIComponent(source)}`;
  } catch { return source; }
}
