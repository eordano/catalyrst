declare global {
  var __UI3_ASSET_BASE__: string | undefined;
}

export const asset = (path: string): string => {
  const root = globalThis.__UI3_ASSET_BASE__;
  if (root) return root + path;
  const url = import.meta.env.BASE_URL + path;
  if (url.startsWith("/") || url.startsWith("http") || typeof document === "undefined") return url;
  return new URL(url, document.baseURI).href;
};
