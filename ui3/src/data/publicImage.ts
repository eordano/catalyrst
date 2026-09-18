import { siteUrl } from "./site";

export function publicImageUrl(value?: string | null): string | undefined {
  if (!value) return undefined;
  try {
    const url = new URL(value);
    if (url.origin === "https://marketing-files.decentraland.org" && /^\/uploads\/[^/]+\.(png|jpg|jpeg|webp|gif)$/i.test(url.pathname)) {
      return siteUrl(`/marketing-files${url.pathname}${url.search}`);
    }
  } catch {  }
  return value;
}
