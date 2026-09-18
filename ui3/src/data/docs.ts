export const DOCS_BASE = "/docs";

function runtimeDocsBase(): string | undefined {
  if (typeof window !== "undefined") {
    const injected = window.__DCL_PUBLIC__?.docsBase;
    if (injected) return injected;
  }
  const proc = (globalThis as { process?: { env?: Record<string, string | undefined> } }).process;
  return proc?.env?.DOCS_BASE || undefined;
}

export function docsBase(): string {
  const raw = (runtimeDocsBase() ?? DOCS_BASE).trim();
  return raw ? raw.replace(/\/+$/, "") : DOCS_BASE;
}

export function docsUrl(path = ""): string {
  const [rawPath = "", hash = ""] = path.split("#", 2);
  const trimmed = rawPath.replace(/^\/+|\/+$/g, "");
  const base = docsBase();
  const url = trimmed ? `${base}/${trimmed}/` : `${base}/`;
  return hash ? `${url}#${hash}` : url;
}
