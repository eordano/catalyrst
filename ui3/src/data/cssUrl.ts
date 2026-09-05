// Remote profile pictures get interpolated into inline styles; a crafted value
// could otherwise close the url("...") token and inject arbitrary declarations.
// Parse, allow only http(s), re-serialize, and refuse anything that could still
// terminate the quoted token.
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
