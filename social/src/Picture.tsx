import { useEffect, useState } from "react";
import { basePath } from "./api";
export const communityImage = (id: string) => `https://assets-cdn.decentraland.org/social/communities/${id}/raw-thumbnail.png`;
const revisions = new Map<string, number>();
export function refreshCommunityPicture(id: string) {
  if (!id) return;
  revisions.set(communityImage(id), Date.now());
  window.dispatchEvent(new CustomEvent('social:community-picture', { detail: communityImage(id) }));
}
export function Picture({ src, fallback = "\u25c7" }: { src?: string; fallback?: string }) {
  const [failed, setFailed] = useState<string>();
  const [revision, setRevision] = useState(() => revisions.get(src || '') || 0);
  useEffect(() => {
    setRevision(revisions.get(src || '') || 0);
    const refresh = (event: Event) => { if ((event as CustomEvent<string>).detail === src) { setFailed(undefined); setRevision(revisions.get(src || '') || 0); } };
    window.addEventListener('social:community-picture', refresh);
    return () => window.removeEventListener('social:community-picture', refresh);
  }, [src]);
  return src && failed !== src ? <img src={`${basePath}api/image?url=${encodeURIComponent(src)}${revision ? `&v=${revision}` : ""}`} alt="" loading="lazy" decoding="async" onError={() => setFailed(src)} /> : <span>{fallback}</span>;
}
