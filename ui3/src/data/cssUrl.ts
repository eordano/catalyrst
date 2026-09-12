export function safeCssUrl(raw: string | null | undefined): string | null {
  if (!raw) return null;
  let url: URL;
  try {
    url = new URL(raw);
  } catch {
    return null;
  }
  if (url.protocol !== "http:" && url.protocol !== "https:") return null;
  const s = url.href;
  if (/["'()\\]/.test(s)) return null;
  return `url("${s}")`;
}
