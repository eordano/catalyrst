import { useState } from "react";
import type { CSSProperties } from "react";
import { SitesChromeMaybe } from "../frames/SitesChrome";
import Modal from "../../components/Modal";
import { suffixLabel, type LabelSuffixProps } from "../../components/labelSuffix";
import "./stprofilephotostab.css";

type VisiblePerson = {
  userName: string;
  userAddress: string;
  isGuest: boolean;
  wearables: string[];
};

type PhotoScene = { name: string; location: { x: string; y: string } };

type PhotoMetadata = {
  userName: string;
  userAddress: string;
  dateTime: string;
  realm: string;
  scene: PhotoScene;
  visiblePeople: VisiblePerson[];
};

export type Photo = { id: string; grad: string; metadata: PhotoMetadata };

export type Profile = {
  address: string;
  name?: string;
  hasClaimedName?: boolean;
  nameColor?: string;
  mutualCount: number;
};

const EMPTY_PROFILE: Profile = { address: "", mutualCount: 0 };

function formatPhotoDate(iso: string) {
  try {
    return new Date(iso).toLocaleString("en-US", {
      weekday: "long",
      year: "numeric",
      month: "long",
      day: "numeric",
      hour: "numeric",
      minute: "2-digit",
    });
  } catch {
    return iso;
  }
}

function truncateAddress(addr: string) {
  return addr ? `${addr.slice(0, 6)}...${addr.slice(-4)}` : "";
}

const WalletIcon = () => (
  <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true" fill="currentColor">
    <path d="M21 7.28V5c0-1.1-.9-2-2-2H5C3.89 3 3 3.9 3 5v14c0 1.1.89 2 2 2h14c1.1 0 2-.9 2-2v-2.28c.59-.35 1-.98 1-1.72V9c0-.74-.41-1.37-1-1.72ZM20 9v6h-7V9h7ZM5 19V5h14v2h-6c-1.1 0-2 .9-2 2v6c0 1.1.9 2 2 2h6v2H5Z" />
    <circle cx="16" cy="12" r="1.5" />
  </svg>
);
const CopyIcon = () => (
  <svg viewBox="0 0 24 24" width="15" height="15" aria-hidden="true" fill="currentColor">
    <path d="M16 1H4c-1.1 0-2 .9-2 2v14h2V3h12V1Zm3 4H8c-1.1 0-2 .9-2 2v14c0 1.1.9 2 2 2h11c1.1 0 2-.9 2-2V7c0-1.1-.9-2-2-2Zm0 16H8V7h11v14Z" />
  </svg>
);
const PersonAddIcon = () => (
  <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true" fill="currentColor">
    <path d="M15 12c2.21 0 4-1.79 4-4s-1.79-4-4-4-4 1.79-4 4 1.79 4 4 4Zm-9-1V8H4v3H1v2h3v3h2v-3h3v-2H6Zm9 3c-2.67 0-8 1.34-8 4v3h16v-3c0-2.66-5.33-4-8-4Z" />
  </svg>
);
const CloseIcon = () => (
  <svg viewBox="0 0 24 24" width="22" height="22" aria-hidden="true" fill="currentColor">
    <path d="M19 6.41 17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12 19 6.41Z" />
  </svg>
);
const LocationIcon = () => (
  <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true" fill="currentColor">
    <path d="M12 2C8.13 2 5 5.13 5 9c0 5.25 7 13 7 13s7-7.75 7-13c0-3.87-3.13-7-7-7Zm0 9.5a2.5 2.5 0 1 1 0-5 2.5 2.5 0 0 1 0 5Z" />
  </svg>
);
const JumpArrowIcon = () => (
  <svg width="18" height="18" viewBox="0 0 24 24" fill="none" aria-hidden="true">
    <path
      d="M18.71 11.07 14.03 6.39c-.83-.83-2.24-.24-2.24.94v1.54H7.26c-.75 0-1.36.59-1.36 1.34v3.56c0 .75.61 1.36 1.36 1.36h4.52v1.54c0 1.18 1.42 1.77 2.24.94l4.68-4.68c.52-.53.52-1.36.01-1.86Z"
      fill="currentColor"
    />
  </svg>
);
const ImageOutlinedIcon = () => (
  <svg viewBox="0 0 24 24" width="56" height="56" aria-hidden="true" fill="currentColor">
    <path d="M21 19V5c0-1.1-.9-2-2-2H5c-1.1 0-2 .9-2 2v14c0 1.1.9 2 2 2h14c1.1 0 2-.9 2-2ZM8.5 13.5l2.5 3.01L14.5 12l4.5 6H5l3.5-4.5Z" />
  </svg>
);

type PhotoModalProps = { image: Photo | null; onClose: () => void };

function PhotoModal({ image, onClose }: PhotoModalProps) {
  if (!image) return null;
  const m = image.metadata;
  return (
    <Modal onClose={onClose} width="100%" className="modal__card--plain stprofilephotostab__dialog" ariaLabelledBy="photo-modal-title">
        <button type="button" className="stprofilephotostab__close" aria-label="Close" onClick={onClose}>
          <CloseIcon />
        </button>
        <div className="stprofilephotostab__body">
          <div className="stprofilephotostab__imagepanel">
            <span className="stprofilephotostab__photo" style={{ background: image.grad }} role="img" aria-label={m.scene.name} />
          </div>
          <div className="stprofilephotostab__meta">
            <div className="stprofilephotostab__metahead">
              <span className="stprofilephotostab__sectitle" id="photo-modal-title">
                information
              </span>
              <span className="stprofilephotostab__dateline">{formatPhotoDate(m.dateTime)}</span>
              {m.userName ? (
                <span className="stprofilephotostab__takenby">
                  Photo taken by{" "}
                  <a
                    className="stprofilephotostab__takenbylink"
                    href={`/profile/${m.userAddress}`}
                    target="_blank"
                    rel="noopener noreferrer"
                  >
                    {m.userName}
                  </a>
                </span>
              ) : null}
            </div>

            <span className="stprofilephotostab__sectitle stprofilephotostab__sectitle--row">place</span>
            <div className="stprofilephotostab__locrow">
              <span className="stprofilephotostab__loclink">
                <LocationIcon />
                <span>
                  {m.scene.name} {m.scene.location.x},{m.scene.location.y}
                </span>
              </span>
              <button type="button" className="stprofilephotostab__jumpin">
                jump in <JumpArrowIcon />
              </button>
            </div>

            {m.visiblePeople.length > 0 ? (
              <div className="stprofilephotostab__people">
                <span className="stprofilephotostab__sectitle stprofilephotostab__sectitle--row">people</span>
                {m.visiblePeople.map((u) => (
                  <div className="stprofilephotostab__person" key={u.userAddress}>
                    <span className="stprofilephotostab__personface u-avatar" style={{ "--sz": "40px" } as CSSProperties & { "--sz": string }} />
                    <div className="stprofilephotostab__personinfo">
                      <span className="stprofilephotostab__personname">{u.userName}</span>
                      <span className="stprofilephotostab__personaddr">{truncateAddress(u.userAddress)}</span>
                    </div>
                  </div>
                ))}
              </div>
            ) : null}
          </div>
        </div>
    </Modal>
  );
}

type StProfilePhotosTabProps = LabelSuffixProps & {
  chrome?: boolean;
  isOwnProfile?: boolean;
  photos?: Photo[];
  profile?: Profile;
};

export default function StProfilePhotosTab({
  labelSuffix,
  chrome = true,
  isOwnProfile = false,
  photos = [],
  profile = EMPTY_PROFILE,
}: StProfilePhotosTabProps) {
  const [openPhoto, setOpenPhoto] = useState<Photo | null>(null);

  const tabs = isOwnProfile
    ? [
        { id: "overview", label: "Overview" },
        { id: "assets", label: "My Assets" },
        { id: "communities", label: "My Communities" },
        { id: "places", label: "My Places" },
        { id: "photos", label: "My Photos" },
        { id: "referral-rewards", label: "Referral Rewards" },
      ]
    : [
        { id: "overview", label: "Overview" },
        { id: "creations", label: "Creations" },
        { id: "communities", label: "Communities" },
        { id: "places", label: "Places" },
        { id: "photos", label: "Photos" },
      ];

  const address = profile.address;
  const displayName = profile.name || profile.address;
  const hasClaimedName = profile.hasClaimedName ?? false;
  const isEmpty = photos.length === 0;

  return (
    <SitesChromeMaybe chrome={chrome} active="legal" overlayNav>
      <div className="stprofilephotostab">
        <div className="stprofilephotostab__card">
          <div className="stprofilephotostab__header">
            <div className="stprofilephotostab__identity">
              <span className="stprofilephotostab__avatar u-avatar" style={{ "--sz": "76px" } as CSSProperties & { "--sz": string }} aria-hidden />
              <div className="stprofilephotostab__nameblock">
                <div className="stprofilephotostab__namerow">
                  <span className="stprofilephotostab__name">{displayName}</span>
                  {hasClaimedName ? (
                    <span className="stprofilephotostab__verified" title="Verified">
                      &#x2713;
                    </span>
                  ) : null}
                </div>
                {address ? (
                  <div className="stprofilephotostab__addrrow">
                    <span className="stprofilephotostab__walleticon">
                      <WalletIcon />
                    </span>
                    <span className="stprofilephotostab__addr">{truncateAddress(address)}</span>
                    <button type="button" className="stprofilephotostab__copy" aria-label="Copy address">
                      <CopyIcon />
                    </button>
                  </div>
                ) : null}
              </div>
            </div>

            <div className="stprofilephotostab__actions">
              {isOwnProfile ? (
                <>
                  <button type="button" className="stprofilephotostab__btn stprofilephotostab__btn--ghost">
                    Friends
                  </button>
                  <button type="button" className="stprofilephotostab__btn stprofilephotostab__btn--ghost">
                    Invite friends
                  </button>
                </>
              ) : (
                <button type="button" className="stprofilephotostab__btn stprofilephotostab__btn--primary">
                  <PersonAddIcon /> Add friend
                </button>
              )}
            </div>
          </div>

          <nav className="stprofilephotostab__tabs" aria-label={suffixLabel("Profile sections", labelSuffix)}>
            {tabs.map((t) => (
              <button
                key={t.id}
                type="button"
                className={"stprofilephotostab__tab" + (t.id === "photos" ? " is-active" : "")}
                aria-current={t.id === "photos" ? "page" : undefined}
              >
                {t.label}
              </button>
            ))}
          </nav>

          <div className="stprofilephotostab__bodyarea">
            {isEmpty ? (
              isOwnProfile ? (
                <div className="stprofilephotostab__empty">
                  <div className="stprofilephotostab__emptyicon">
                    <ImageOutlinedIcon />
                  </div>
                  <div className="stprofilephotostab__emptybody">
                    <p className="stprofilephotostab__emptytitle">No photos yet</p>
                    <p className="stprofilephotostab__emptysub">
                      Jump in and start collecting memories! Press 'C' to open the Camera and 'Space bar' to snap a
                      photo.
                    </p>
                    <button type="button" className="stprofilephotostab__emptycta">
                      Jump in <JumpArrowIcon />
                    </button>
                  </div>
                </div>
              ) : (
                <p className="stprofilephotostab__emptymember">This member has not taken any photos yet.</p>
              )
            ) : (
              <>
                <p className="stprofilephotostab__count">{photos.length} photos</p>
                <div className="stprofilephotostab__grid">
                  {photos.map((image) => (
                    <button
                      key={image.id}
                      type="button"
                      className="stprofilephotostab__photocard"
                      onClick={() => setOpenPhoto(image)}
                    >
                      <span
                        className="stprofilephotostab__photoimg"
                        style={{ background: image.grad }}
                        role="img"
                        aria-label={image.metadata?.scene?.name ?? "Snapshot"}
                      />
                    </button>
                  ))}
                </div>
              </>
            )}
          </div>
        </div>
      </div>

      {openPhoto ? <PhotoModal image={openPhoto} onClose={() => setOpenPhoto(null)} /> : null}
    </SitesChromeMaybe>
  );
}
