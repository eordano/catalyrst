import type { ReactNode } from "react";
import { useState } from "react";
import { SitesChromeMaybe } from "../frames/SitesChrome";
import Modal from "../../components/Modal";
import { Copy } from "../../atoms/icons";
import { suffixLabel, type LabelSuffixProps } from "../../components/labelSuffix";
import "./stprofileplacestab.css";

const C = "stprofileplacestab";

export type Profile = {
  address: string;
  name?: string;
  hasClaimedName?: boolean;
  nameColor?: string;
  mutualCount: number;
};

export type Place = {
  id: string;
  title: string;
  description?: string;
  image?: string;
  base_position?: string;
  positions?: string[];
  world?: boolean;
  world_name?: string;
  likes?: number;
  favorites?: number;
  user_count?: number;
  owner?: string;
  contact_name?: string;
};

type TabItem = { id: string; label: string };

type EmptyView = "owner" | "favorites" | "member";

const EMPTY_PROFILE: Profile = { address: "", mutualCount: 0 };

const CREATOR_FACE_COLOR = "#FF8362";

const MEMBER_TABS: TabItem[] = [
  { id: "overview", label: "Overview" },
  { id: "creations", label: "Creations" },
  { id: "communities", label: "Communities" },
  { id: "places", label: "Places" },
  { id: "photos", label: "Photos" },
];
const OWN_TABS: TabItem[] = [
  { id: "overview", label: "Overview" },
  { id: "assets", label: "My Assets" },
  { id: "communities", label: "My Communities" },
  { id: "places", label: "My Places" },
  { id: "photos", label: "My Photos" },
  { id: "referral-rewards", label: "Referral Rewards" },
];

function truncateAddress(addr: string) {
  return `${addr.slice(0, 6)}...${addr.slice(-4)}`;
}

function countLabel(n: number) {
  return `${n} places`;
}

function placeCoordinates(p: Place) {
  return p.world ? p.world_name : p.base_position;
}

const WalletGlyph = () => (
  <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
    <path d="M3 7a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2v1h1a1 1 0 0 1 1 1v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7Zm14 7a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3Z" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinejoin="round" />
  </svg>
);
const VerifiedGlyph = () => (
  <svg viewBox="0 0 24 24" width="11" height="11" aria-hidden="true">
    <path d="m6 12 4 4 8-9" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round" />
  </svg>
);
const PersonAddGlyph = () => (
  <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
    <circle cx="9" cy="8" r="3.4" fill="none" stroke="currentColor" strokeWidth="1.7" />
    <path d="M3.5 19a5.5 5.5 0 0 1 11 0M18 8v6M15 11h6" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" />
  </svg>
);
const MoreGlyph = () => (
  <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
    <circle cx="12" cy="5" r="1.6" fill="currentColor" />
    <circle cx="12" cy="12" r="1.6" fill="currentColor" />
    <circle cx="12" cy="19" r="1.6" fill="currentColor" />
  </svg>
);
const GlobeBtnGlyph = () => (
  <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
    <circle cx="12" cy="12" r="9" fill="none" stroke="currentColor" strokeWidth="1.7" />
    <path d="M3 12h18M12 3c2.6 2.6 2.6 15.4 0 18M12 3c-2.6 2.6-2.6 15.4 0 18" fill="none" stroke="currentColor" strokeWidth="1.7" />
  </svg>
);
const PlacePinGlyph = ({ size = 18 }: { size?: number }) => (
  <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true">
    <path d="M12 2a7 7 0 0 0-7 7c0 5 7 13 7 13s7-8 7-13a7 7 0 0 0-7-7Z" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinejoin="round" />
    <circle cx="12" cy="9" r="2.4" fill="none" stroke="currentColor" strokeWidth="1.7" />
  </svg>
);
const HeartGlyph = ({ size = 18 }: { size?: number }) => (
  <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true">
    <path d="M12 20S3.5 14.5 3.5 8.6A4.5 4.5 0 0 1 12 7a4.5 4.5 0 0 1 8.5 1.6C20.5 14.5 12 20 12 20Z" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinejoin="round" />
  </svg>
);
const FavoriteFilledGlyph = ({ size = 18 }: { size?: number }) => (
  <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true">
    <path d="M12 20S3.5 14.5 3.5 8.6A4.5 4.5 0 0 1 12 7a4.5 4.5 0 0 1 8.5 1.6C20.5 14.5 12 20 12 20Z" fill="currentColor" />
  </svg>
);
const PeopleGlyph = ({ size = 18 }: { size?: number }) => (
  <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true">
    <circle cx="9" cy="8" r="3.2" fill="none" stroke="currentColor" strokeWidth="1.7" />
    <path d="M3.5 19a5.5 5.5 0 0 1 11 0M16 5.5a3 3 0 0 1 0 5.8M20.5 19a5.2 5.2 0 0 0-4-5" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" />
  </svg>
);
const PublicGlyph = ({ size = 18 }: { size?: number }) => (
  <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true">
    <circle cx="12" cy="12" r="9" fill="none" stroke="currentColor" strokeWidth="1.7" />
    <path d="M3 12h18M12 3c2.6 2.6 2.6 15.4 0 18M12 3c-2.6 2.6-2.6 15.4 0 18" fill="none" stroke="currentColor" strokeWidth="1.7" />
  </svg>
);
const CloseGlyph = () => (
  <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
    <path d="M6 6l12 12M18 6 6 18" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
  </svg>
);
const CopyLinkGlyph = () => (
  <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
    <rect x="9" y="9" width="11" height="11" rx="2" fill="none" stroke="currentColor" strokeWidth="1.7" />
    <path d="M5 15V5a2 2 0 0 1 2-2h8" fill="none" stroke="currentColor" strokeWidth="1.7" />
  </svg>
);
const JumpInGlyph = () => (
  <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
    <path d="M5 12h12M13 7l5 5-5 5" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
  </svg>
);
const JumpInBadgeIcon = () => (
  <svg width="24" height="24" viewBox="0 0 24 24" fill="none" aria-hidden="true">
    <rect x="0.75" y="0.75" width="22.5" height="22.5" rx="7.25" stroke="currentColor" strokeOpacity="0.7" strokeWidth="1.5" />
    <path
      d="M18.7 11.06 14.03 6.39c-.83-.83-2.24-.24-2.24.93v1.55h-4.5c-.75 0-1.36.6-1.36 1.35v3.56c0 .75.61 1.36 1.36 1.36h4.5v1.54c0 1.18 1.42 1.77 2.24.94l4.67-4.68c.53-.53.53-1.36.02-1.87Z"
      fill="currentColor"
    />
  </svg>
);

type PlaceCardProps = { place: Place; onOpen: () => void };

function PlaceCard({ place, onOpen }: PlaceCardProps) {
  const coords = placeCoordinates(place);
  const isWorld = Boolean(place.world);
  return (
    <article className={`${C}__card2`}>
      <button type="button" className={`${C}__action`} onClick={onOpen}>
        <span className={`${C}__media`}>
          <span
            className={`${C}__mediaimg`}
            style={place.image ? { backgroundImage: place.image } : undefined}
            aria-hidden="true"
          />
        </span>
        <span className={`${C}__content`}>
          <span className={`${C}__inner`}>
            <span className={`${C}__info`}>
              <span className={`${C}__title2`}>{place.title}</span>
              <span className={`${C}__metarow`}>
                <span className={`${C}__by`}>
                  <span className={`${C}__byface`} style={{ background: CREATOR_FACE_COLOR }} aria-hidden="true" />
                  <span className={`${C}__bytext`}>
                    by <span className={`${C}__byname`}>{place.contact_name}</span>
                  </span>
                </span>
                {coords ? (
                  <span className={`${C}__loc`}>
                    {isWorld ? <PublicGlyph size={16} /> : <PlacePinGlyph size={16} />}
                    <span className={`${C}__loctext`}>{coords}</span>
                  </span>
                ) : null}
              </span>
            </span>
            <span className={`${C}__jumpwrap`}>
              <span className={`${C}__jump`}>
                <JumpInGlyph />
                Jump in
              </span>
            </span>
          </span>
        </span>
      </button>
    </article>
  );
}

type PlaceDetailModalProps = { place: Place | null; onClose: () => void };

function PlaceDetailModal({ place, onClose }: PlaceDetailModalProps) {
  if (!place) return null;
  const isWorld = Boolean(place.world);
  const coords = place.base_position ?? (place.positions && place.positions[0]) ?? "0,0";
  const locationLabel = isWorld && place.world_name ? place.world_name : coords;
  const favorites = place.favorites ?? place.likes ?? 0;
  const userCount = place.user_count ?? 0;
  const showStats = favorites > 0 || userCount > 0;
  const hasDescription = Boolean(place.description);

  return (
    <Modal onClose={onClose} width="100%" className={`modal__card--plain ${C}__dialog`} ariaLabelledBy="place-detail-title">
        <div className={`${C}__hero`}>
          {place.image ? (
            <span className={`${C}__heroimg`} style={{ backgroundImage: place.image }} aria-hidden="true" />
          ) : null}
          <span className={`${C}__herooverlay`} aria-hidden="true" />
          <button type="button" className={`${C}__close`} aria-label="Close" onClick={onClose}>
            <CloseGlyph />
          </button>
          <div className={`${C}__herocontent`}>
            <h2 id="place-detail-title" className={`${C}__modaltitle`}>
              {place.title}
            </h2>
            <div className={`${C}__creator`}>
              <span className={`${C}__creatorface`} style={{ background: CREATOR_FACE_COLOR }} aria-hidden="true" />
              <span className={`${C}__creatorname`}>
                By <span className={`${C}__creatorhi`}>{place.contact_name}</span>
              </span>
            </div>
            <div className={`${C}__actions2`}>
              <button type="button" className={`${C}__jumpcta`}>
                <JumpInGlyph />
                Jump In
              </button>
              <button type="button" className={`${C}__copylink`} aria-label="Copy link">
                <CopyLinkGlyph />
              </button>
            </div>
          </div>
        </div>

        <div className={`${C}__contentsection`}>
          {hasDescription ? (
            <>
              <p className={`${C}__seclabel`}>About this place</p>
              <p className={`${C}__desc`}>{place.description}</p>
              <span className={`${C}__divider`} aria-hidden="true" />
            </>
          ) : null}
          <p className={`${C}__seclabel`}>{isWorld ? "World" : "Location"}</p>
          <div className={`${C}__metarow2`}>
            <span className={`${C}__metatext`}>
              {isWorld ? <PublicGlyph /> : <PlacePinGlyph />}
              {locationLabel}
            </span>
          </div>
          {showStats ? (
            <>
              <span className={`${C}__divider`} aria-hidden="true" />
              <div className={`${C}__metarow2`}>
                {userCount > 0 ? (
                  <span className={`${C}__metatext`}>
                    <PeopleGlyph />
                    {userCount === 1 ? "1 person here now" : `${userCount} people here now`}
                  </span>
                ) : null}
                {favorites > 0 ? (
                  <span className={`${C}__metatext`}>
                    <FavoriteFilledGlyph />
                    {favorites === 1 ? "1 favorite" : `${favorites} favorites`}
                  </span>
                ) : null}
              </div>
            </>
          ) : null}
        </div>
    </Modal>
  );
}

type EmptyAction = {
  label: ReactNode;
  href?: string;
  onClick?: () => void;
  startIcon?: ReactNode;
  endIcon?: ReactNode;
};

type EmptyStateProps = {
  icon: ReactNode;
  title: ReactNode;
  subtitle?: ReactNode;
  action?: EmptyAction;
};

function EmptyState({ icon, title, subtitle, action }: EmptyStateProps) {
  return (
    <div className={`${C}__emptybox`}>
      <span className={`${C}__emptyicon`}>{icon}</span>
      <div className={`${C}__emptybody`}>
        <h3 className={`${C}__emptytitle`}>{title}</h3>
        {subtitle ? <p className={`${C}__emptysub`}>{subtitle}</p> : null}
        {action ? (
          action.href ? (
            <a
              className={`${C}__emptycta`}
              href={action.href}
              target="_blank"
              rel="noopener noreferrer"
            >
              {action.startIcon ? <span className={`${C}__ctaicon`}>{action.startIcon}</span> : null}
              {action.label}
              {action.endIcon ? <span className={`${C}__ctaicon`}>{action.endIcon}</span> : null}
            </a>
          ) : (
            <button type="button" className={`${C}__emptycta`} onClick={action.onClick}>
              {action.startIcon ? <span className={`${C}__ctaicon`}>{action.startIcon}</span> : null}
              {action.label}
              {action.endIcon ? <span className={`${C}__ctaicon`}>{action.endIcon}</span> : null}
            </button>
          )
        ) : null}
      </div>
    </div>
  );
}

type StProfilePlacesTabProps = LabelSuffixProps & {
  chrome?: boolean;
  profile?: Profile;
  isOwnProfile?: boolean;
  loading?: boolean;
  places?: Place[];
  emptyView?: EmptyView | null;
};

export default function StProfilePlacesTab({
  labelSuffix,
  chrome = true,
  profile = EMPTY_PROFILE,
  isOwnProfile = false,
  loading = false,
  places = [],
  emptyView = null,
}: StProfilePlacesTabProps) {
  const initial = (profile.name || profile.address).charAt(0).toUpperCase();
  const tabs = isOwnProfile ? OWN_TABS : MEMBER_TABS;
  const hasClaimedName = profile.hasClaimedName ?? false;

  const [view, setView] = useState<"places" | "favorites">("places");
  const showFavorites = isOwnProfile && view === "favorites";

  const list = emptyView || showFavorites ? [] : places;
  const isEmpty = list.length === 0;

  const [openPlace, setOpenPlace] = useState<Place | null>(null);

  const filters = isOwnProfile ? (
    <div className={`${C}__filters`}>
      <button
        type="button"
        className={`${C}__chip` + (view === "places" ? " is-active" : "")}
        aria-pressed={view === "places"}
        onClick={() => setView("places")}
      >
        <span className={`${C}__chipicon`}><PlacePinGlyph size={18} /></span>
        My Places
      </button>
      <button
        type="button"
        className={`${C}__chip` + (view === "favorites" ? " is-active" : "")}
        aria-pressed={view === "favorites"}
        onClick={() => setView("favorites")}
      >
        <span className={`${C}__chipicon`}><HeartGlyph size={18} /></span>
        Favourites
      </button>
    </div>
  ) : null;

  const emptyKind: EmptyView | null =
    emptyView ?? (isEmpty ? (showFavorites ? "favorites" : isOwnProfile ? "owner" : "member") : null);

  let body: ReactNode;
  if (loading) {
    body = (
      <div className={`${C}__loadingrow`}>
        <span className={`${C}__spinner`} role="status" aria-label="Loading places" />
      </div>
    );
  } else if (isEmpty) {
    if (emptyKind === "favorites") {
      body = (
        <EmptyState
          icon={<HeartGlyph size={56} />}
          title="Nothing saved yet"
          subtitle="Add favorite Places by tapping the heart on a place card or the map in Decentraland. You'll find them all here."
          action={{ label: "Explore places", onClick: () => {}, endIcon: <JumpInBadgeIcon /> }}
        />
      );
    } else if (emptyKind === "owner") {
      body = (
        <EmptyState
          icon={<PlacePinGlyph size={56} />}
          title={"Nothing here yet\u{2014}but that's easy to change"}
          subtitle="My Places shows the spaces you own or manage in Decentraland. The easiest way to start is with a NAME, which unlocks your own World."
          action={{ label: "Get a name", href: "https://decentraland.org/builder/names", startIcon: <VerifiedGlyph /> }}
        />
      );
    } else {
      body = <p className={`${C}__emptymember`}>This member has not registered any places yet.</p>;
    }
  } else {
    body = (
      <>
        <p className={`${C}__count`}>{countLabel(list.length)}</p>
        <div className={`${C}__grid`}>
          {list.map((place) => (
            <PlaceCard key={place.id} place={place} onOpen={() => setOpenPlace(place)} />
          ))}
        </div>
      </>
    );
  }

  return (
    <SitesChromeMaybe chrome={chrome} active="legal" overlayNav>
      <div className={C}>
        <div className={`${C}__contentwrap`}>
          <div className={`${C}__profilecard`}>
            <div className={`${C}__header`}>
              <div className={`${C}__identity`}>
                <span className={`${C}__avatar`} style={{ background: profile.nameColor }} aria-hidden="true">
                  <span className={`${C}__avatarinitial`}>{initial}</span>
                </span>
                <div className={`${C}__nameblock`}>
                  <div className={`${C}__namerow`}>
                    <span className={`${C}__name`} style={{ color: profile.nameColor }}>
                      {profile.name}
                    </span>
                    {hasClaimedName ? (
                      <span className={`${C}__verified`} style={{ background: profile.nameColor }} title="Verified">
                        <VerifiedGlyph />
                      </span>
                    ) : profile.address ? (
                      <span className={`${C}__discriminator`}>#{profile.address.slice(-4)}</span>
                    ) : null}
                  </div>
                  {profile.address ? (
                    <div className={`${C}__addrrow`}>
                      <span className={`${C}__walleticon`}><WalletGlyph /></span>
                      <span className={`${C}__addr`}>{truncateAddress(profile.address)}</span>
                      <button type="button" className={`${C}__copy`} aria-label="Copy address">
                        <Copy size={16} />
                      </button>
                    </div>
                  ) : null}
                </div>
              </div>

              <div className={`${C}__actionsblock`}>
                {isOwnProfile ? (
                  <button type="button" className={`${C}__namecta`}>
                    {hasClaimedName ? <GlobeBtnGlyph /> : <VerifiedGlyph />}
                    {hasClaimedName ? "Manage World" : "Get a Unique Name"}
                  </button>
                ) : (
                  <>
                    {profile.mutualCount > 0 ? (
                      <button type="button" className={`${C}__mutual`} aria-label={`${profile.mutualCount} Mutual`}>
                        <span className={`${C}__mutualstack`}>
                          {Array.from({ length: Math.min(3, profile.mutualCount) }).map((_, i) => (
                            <span
                              key={i}
                              className={`${C}__mutualpic`}
                              style={{ background: ["#73d3d3", "#ff8362", "#b05cff"][i % 3], marginLeft: i ? -8 : 0 }}
                              aria-hidden="true"
                            />
                          ))}
                        </span>
                        <span className={`${C}__mutualtext`}>
                          <strong>{profile.mutualCount}</strong> Mutual
                        </span>
                      </button>
                    ) : null}
                    <button type="button" className={`${C}__friendcta`}>
                      <PersonAddGlyph />
                      Add friend
                    </button>
                    <button type="button" className={`${C}__more`} aria-label="More actions">
                      <MoreGlyph />
                    </button>
                  </>
                )}
              </div>
            </div>

            <nav className={`${C}__tabs`} aria-label={suffixLabel("Profile sections", labelSuffix)}>
              {tabs.map((tab) => (
                <button
                  key={tab.id}
                  type="button"
                  className={`${C}__tab` + (tab.id === "places" ? " is-active" : "")}
                  aria-current={tab.id === "places" ? "page" : undefined}
                >
                  {tab.label}
                  {tab.id === "places" ? <span className={`${C}__tabbar`} aria-hidden="true" /> : null}
                </button>
              ))}
            </nav>

            <div className={`${C}__body`}>
              {filters}
              {body}
            </div>
          </div>
        </div>
      </div>

      <PlaceDetailModal place={openPlace} onClose={() => setOpenPlace(null)} />
    </SitesChromeMaybe>
  );
}
