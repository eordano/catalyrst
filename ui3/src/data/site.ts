type SiteWindow = Window & typeof globalThis & { __SITE_BASE__?: string };

const SITE_DEFAULT_BASE = "https://catalyst.example.com";

export function siteBase(override?: string): string {
  const fromEnv: string | undefined =
    typeof import.meta !== "undefined" ? import.meta.env?.VITE_SITE_URL : undefined;
  const fromWindow =
    typeof window !== "undefined" ? (window as SiteWindow).__SITE_BASE__ : undefined;
  return (override ?? fromEnv ?? fromWindow ?? SITE_DEFAULT_BASE).replace(/\/$/, "");
}

export function siteUrl(path = ""): string {
  if (!path) return siteBase();
  return siteBase() + (path.startsWith("/") ? path : `/${path}`);
}

export function siteHost(): string {
  return new URL(siteBase()).host;
}
