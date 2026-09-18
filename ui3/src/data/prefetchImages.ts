const warmed = new Map<string, { image: HTMLImageElement; at: number }>();

export function prefetchImages(urls: (string | null | undefined)[]) {
  if (typeof Image === "undefined") return;
  for (const url of new Set(urls.filter((url): url is string => Boolean(url)))) {
    if (Date.now() - (warmed.get(url)?.at ?? 0) < 5 * 60_000) continue;
    const image = new Image();
    image.fetchPriority = "low";
    image.decoding = "async";
    const entry = { image, at: Date.now() };
    image.onerror = () => { if (warmed.get(url) === entry) warmed.delete(url); };
    warmed.delete(url);
    warmed.set(url, entry);
    image.src = url;
    if (warmed.size > 256) warmed.delete(warmed.keys().next().value!);
  }
}
