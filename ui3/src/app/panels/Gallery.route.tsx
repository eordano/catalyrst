import Reel from "../../explorer/pages/Reel";
export { prefetchReel as prefetch } from "../../data/catalyst/reel";

export default function GalleryPanel({ embedded = true }: { embedded?: boolean }) {
  return <Reel embedded={embedded} />;
}
