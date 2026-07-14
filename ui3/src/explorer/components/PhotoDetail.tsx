// Full-screen photo viewer (lightbox) for one camera-reel photo: large image with
// prev/next navigation, an info sidebar (date - scene+coords - people in the shot),
// and the reel actions (jump in - download - copy link - share to X - two-step delete).

import { useEffect, useRef, useState } from "react";
import { Avatar } from "../../atoms/primitives";
import Button from "../../atoms/Button";
import { useDialogKeys } from "../../components/useDialogKeys";
import "./photodetail-view.css";

export type ReelPerson = {
  userName: string;
  userAddress: string;
  wearables?: string[];
  isGuest?: boolean;
};

export type ReelPhotoMeta = {
  userName?: string;
  userAddress?: string;
  dateTime?: string;
  realm?: string;
  scene?: { name?: string; location?: { x?: string; y?: string } };
  visiblePeople?: ReelPerson[];
  placeId?: string;
};

export type ReelPhoto = {
  id: string;
  url: string;
  thumbnailUrl?: string;
  isPublic?: boolean;
  metadata?: ReelPhotoMeta;
};

/** Parse a camera-reel `dateTime` (unix seconds, unix ms, or ISO) to epoch ms. */
export function photoTime(dateTime: string | undefined): number {
  if (!dateTime) return 0;
  if (/^\d+$/.test(dateTime)) {
    const n = Number(dateTime);
    return n < 1e12 ? n * 1000 : n;
  }
  const t = Date.parse(dateTime);
  return Number.isNaN(t) ? 0 : t;
}

function formatDate(dateTime: string | undefined): string {
  const ms = photoTime(dateTime);
  if (!ms) return "";
  return new Date(ms).toLocaleDateString("en-US", { month: "long", day: "numeric", year: "numeric" });
}

const Glyph = ({ d }: { d: string }): React.JSX.Element => (
  <svg viewBox="0 0 24 24" width="16" height="16" fill="currentColor" aria-hidden="true">
    <path d={d} />
  </svg>
);
const ICON = {
  jump: "M10 17l5-5-5-5v10z",
  download: "M19 9h-4V3H9v6H5l7 7 7-7zM5 18v2h14v-2H5z",
  link: "M3.9 12a3.1 3.1 0 013.1-3.1h4V7H7a5 5 0 100 10h4v-1.9H7A3.1 3.1 0 013.9 12zM17 7h-4v1.9h4a3.1 3.1 0 010 6.2h-4V17h4a5 5 0 100-10zM8 11h8v2H8z",
  share: "M18 16.08c-.76 0-1.44.3-1.96.77L8.91 12.7c.05-.23.09-.46.09-.7s-.04-.47-.09-.7l7.05-4.11c.54.5 1.25.81 2.04.81a3 3 0 100-6 3 3 0 00-3 3c0 .24.04.47.09.7L8.04 9.81A3 3 0 003 12a3 3 0 003 3 3 3 0 001.96-.73l7.12 4.16c-.05.21-.08.43-.08.65a2.92 2.92 0 105.84 0 3 3 0 00-2.84-2.65z",
  trash: "M6 19c0 1.1.9 2 2 2h8c1.1 0 2-.9 2-2V7H6v12zM19 4h-3.5l-1-1h-5l-1 1H5v2h14V4z",
} as const;

type PhotoDetailProps = {
  photos: ReelPhoto[];
  index: number;
  /** The reel is the local player's own, so delete is allowed. */
  isSelf: boolean;
  onIndex: (i: number) => void;
  onClose: () => void;
  onTeleport: (x: number, y: number) => void;
  onDelete?: (id: string) => void;
  onViewPerson?: (address: string) => void;
};

export default function PhotoDetail({
  photos,
  index,
  isSelf,
  onIndex,
  onClose,
  onTeleport,
  onDelete,
  onViewPerson,
}: PhotoDetailProps): React.JSX.Element | null {
  const photo = photos[index];
  const [copied, setCopied] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const hasPrev = index > 0;
  const hasNext = index < photos.length - 1;

  const cardRef = useRef<HTMLDivElement>(null);
  useDialogKeys(cardRef, onClose);

  useEffect(() => {
    setCopied(false);
    setConfirmDelete(false);
  }, [photo?.id]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent): void => {
      if (e.key === "ArrowLeft" && hasPrev) onIndex(index - 1);
      else if (e.key === "ArrowRight" && hasNext) onIndex(index + 1);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [index, hasPrev, hasNext, onIndex]);

  if (!photo) return null;

  const copyLink = (): void => {
    navigator.clipboard
      ?.writeText(photo.url)
      .then(() => setCopied(true))
      .catch(() => console.warn("[reel] clipboard write failed"));
  };
  const shareToX = (): void => {
    const text = encodeURIComponent("Check out my photo from Decentraland \u{1F44B}");
    const url = encodeURIComponent(photo.url);
    window.open(`https://x.com/intent/tweet?text=${text}&url=${url}`, "_blank", "noopener");
  };

  const date = formatDate(photo.metadata?.dateTime);
  const place = photo.metadata?.scene?.name;
  const px = Number(photo.metadata?.scene?.location?.x);
  const py = Number(photo.metadata?.scene?.location?.y);
  const hasCoords = Number.isFinite(px) && Number.isFinite(py);
  const people = photo.metadata?.visiblePeople ?? [];

  return (
    <div
      className="rpd__viewer"
      role="dialog"
      aria-modal="true"
      aria-label="Photo"
      tabIndex={-1}
      ref={cardRef}
    >
      <button type="button" className="rpd__close" aria-label="Close" onClick={onClose}>
        &#xD7;
      </button>

      <div className="rpd__stage">
        <button
          type="button"
          className="rpd__nav rpd__navPrev"
          aria-label="Previous photo"
          disabled={!hasPrev}
          onClick={() => hasPrev && onIndex(index - 1)}
        >
          &#x2039;
        </button>
        <img className="rpd__stageImg" src={photo.url} alt="" />
        <button
          type="button"
          className="rpd__nav rpd__navNext"
          aria-label="Next photo"
          disabled={!hasNext}
          onClick={() => hasNext && onIndex(index + 1)}
        >
          &#x203A;
        </button>
      </div>

      <aside className="rpd__info">
        <div className="rpd__actions">
          {hasCoords && (
            <Button size="sm" variant="ghost" onClick={() => onTeleport(px, py)}>
              <Glyph d={ICON.jump} /> Jump In
            </Button>
          )}
          <a className="rpd__actionLink" href={photo.url} target="_blank" rel="noopener noreferrer" download>
            <Glyph d={ICON.download} /> Download
          </a>
          <Button size="sm" variant="ghost" onClick={copyLink}>
            <Glyph d={ICON.link} /> {copied ? "Copied!" : "Copy link"}
          </Button>
          <Button size="sm" variant="ghost" onClick={shareToX}>
            <Glyph d={ICON.share} /> Share
          </Button>
          {isSelf && onDelete && (
            <Button
              size="sm"
              variant="ghost"
              className="rpd__deleteBtn"
              onClick={() => (confirmDelete ? onDelete(photo.id) : setConfirmDelete(true))}
            >
              <Glyph d={ICON.trash} /> {confirmDelete ? "Confirm?" : "Delete"}
            </Button>
          )}
        </div>

        {date && (
          <div className="rpd__infoBlock">
            <span className="rpd__infoLabel">Date</span>
            <span className="rpd__infoValue">{date}</span>
          </div>
        )}

        {place && (
          <div className="rpd__infoBlock">
            <span className="rpd__infoLabel">Scene</span>
            <span className="rpd__infoValue">
              {place}
              {hasCoords && (
                <span className="rpd__coords">
                  {" "}
                  &#xB7; {px}, {py}
                </span>
              )}
            </span>
          </div>
        )}

        {people.length > 0 && (
          <div className="rpd__infoBlock">
            <span className="rpd__infoLabel">People in this photo</span>
            <div className="rpd__people">
              {people.map((person) => (
                <button
                  key={person.userAddress}
                  type="button"
                  className="rpd__person"
                  onClick={() => onViewPerson?.(person.userAddress)}
                >
                  <Avatar name={person.userName || person.userAddress} size={28} />
                  <span className="rpd__personName">
                    {person.userName || `${person.userAddress.slice(0, 6)}\u{2026}`}
                  </span>
                </button>
              ))}
            </div>
          </div>
        )}
      </aside>
    </div>
  );
}
