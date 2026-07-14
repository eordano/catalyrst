import { useEffect, useRef, useState } from "react";

import { AvatarStage } from "../../explorer/components/AvatarPreview";
import { rarityColor } from "../../atoms/primitives";
import { useDialogKeys } from "../../components/useDialogKeys";
import "../../explorer/components/tabs.css";
import "../../explorer/pages/passport.css";
import "../../explorer/pages/passportphotos.css";
import "../../explorer/pages/badgesdetail.css";
import "../../explorer/pages/photodetail.css";
import "./clientstage.css";

export type PassportEquippedItem = {
  name: string;
  rarity: string;
  category: string;
  cat: string;
};
export type PassportLink = { title: string; url: string };
export type PassportInfoField = { key: string; label: string; value: string };

export type PassportProfile = {
  address: string;
  name: string;
  tag: string;
  hasClaimedName: boolean;
  nameColor: string;
  description: string;
  links: PassportLink[];
  info: PassportInfoField[];
  equipped: PassportEquippedItem[];
};

export type PassportBadgeMedallion = {
  id: string;
  name: string;
  tier: string;
  tint: string;
  shape: string;
};
export type PassportBadgeCard = {
  id: string;
  name: string;
  tier: string;
  unlocked: boolean;
  isNew: boolean;
  completedAt: string | null;
};
export type PassportBadgeSection = {
  id: string;
  label: string;
  badges: PassportBadgeCard[];
};
export type PassportBadges = {
  /** null when the category list was never read -- not "there are none". */
  categories: string[] | null;
  earned: PassportBadgeMedallion[];
  sections: PassportBadgeSection[];
  /** Set when the badge read failed; `earned` is then empty and means nothing. */
  unavailable?: string | null;
};

export type PassportPhotoPerson = { name: string; tag: string; wearables: string[] };
export type PassportPhoto = {
  id: string;
  dateTime: string;
  hue: number;
  place: { name: string; x: string; y: string } | null;
  people: PassportPhotoPerson[];
};

export type PassportData = {
  profile: PassportProfile;
  badges: PassportBadges;
  photos: PassportPhoto[];
  /** Set when the photo read failed; `photos` is then empty and means nothing. */
  photosUnavailable?: string | null;
  /** Set when the profile read failed; the profile fields are placeholders. */
  profileUnavailable?: string | null;
};

export type PassportTab = "overview" | "badges" | "photos";

const TABS: { id: PassportTab; label: string }[] = [
  { id: "overview", label: "Overview" },
  { id: "badges", label: "Badges" },
  { id: "photos", label: "Photos" },
];

const TIER_TINT: Record<string, string> = {
  bronze: "#cd7f32",
  silver: "#c7ccd1",
  gold: "#ffc647",
  platinum: "#6cd0e0",
  diamond: "#b9f2ff",
};

export type PassportPanelProps = {
  data: PassportData;
  tab: PassportTab;
  openPhotoId: string | null;
  self: boolean;
  onTab: (tab: PassportTab) => void;
  onOpenPhoto: (id: string | null) => void;
  onClose: () => void;
  onMounted?: (address: string, self: boolean) => void;
  onTabViewed?: (tab: PassportTab) => void;
  onLinkClicked?: (url: string) => void;
  onItemClicked?: (name: string, category: string) => void;
  onPhotoOpened?: (id: string) => void;
  onClaimName?: () => void;
  onEdit?: (field: string) => void;
};

export default function PassportPanel(props: PassportPanelProps) {
  const {
    data,
    tab,
    openPhotoId,
    self,
    onTab,
    onOpenPhoto,
    onClose,
    onMounted,
    onTabViewed,
    onLinkClicked,
    onItemClicked,
    onPhotoOpened,
    onClaimName,
    onEdit,
  } = props;
  const { profile, badges, photos } = data;

  const mounted = useRef(false);
  useEffect(() => {
    if (mounted.current) return;
    mounted.current = true;
    onMounted?.(profile.address, self);
  }, [onMounted, profile.address, self]);

  const lastTab = useRef<PassportTab | null>(null);
  useEffect(() => {
    if (tab !== lastTab.current) onTabViewed?.(tab);
    lastTab.current = tab;
  }, [tab, onTabViewed]);

  const lastPhoto = useRef<string | null>(null);
  useEffect(() => {
    if (openPhotoId && openPhotoId !== lastPhoto.current) onPhotoOpened?.(openPhotoId);
    lastPhoto.current = openPhotoId;
  }, [openPhotoId, onPhotoOpened]);

  const openPhoto = openPhotoId
    ? photos.find((p) => p.id === openPhotoId) ?? null
    : null;

  return (
    <div className="client-stage">
      <div className="client-canvas" aria-hidden="true" />

      <div className="bv-overlay">
        <div className="ep__backdrop">
          <div className="ps">
            <button className="ep__close ps__close" aria-label="Close" onClick={onClose}>
              &#xD7;
            </button>

            <div className="ps__preview">
              <AvatarStage profile={profile.address || undefined} />
            </div>

            <div className="ps__main">
              <Header
                profile={profile}
                self={self}
                onClaimName={onClaimName}
                onEdit={onEdit}
              />

              {data.profileUnavailable && (
                <p className="ps__failed" role="alert">
                  We couldn't load this profile: {data.profileUnavailable}. The
                  name and details below are placeholders, not this player's.
                </p>
              )}

              <Tabs tab={tab} onTab={onTab} />

              {tab === "overview" && (
                <Overview
                  data={data}
                  self={self}
                  onLinkClicked={onLinkClicked}
                  onItemClicked={onItemClicked}
                  onEdit={onEdit}
                />
              )}
              {tab === "badges" && <Badges badges={badges} />}
              {tab === "photos" && (
                <Photos
                  photos={photos}
                  unavailable={data.photosUnavailable ?? null}
                  onOpenPhoto={onOpenPhoto}
                />
              )}
            </div>
          </div>

          {openPhoto && (
            <PhotoLightbox
              photo={openPhoto}
              onClose={() => onOpenPhoto(null)}
            />
          )}
        </div>
      </div>

      <noscript>
        <p style={{ color: "#fff", padding: 16 }}>
          Enable JavaScript to switch passport tabs and open photos.
        </p>
      </noscript>
    </div>
  );
}

function shortAddr(addr: string): string {
  if (addr.length <= 12) return addr;
  return `${addr.slice(0, 6)}\u{2026}${addr.slice(-4)}`;
}

function Header({
  profile,
  self,
  onClaimName,
  onEdit,
}: {
  profile: PassportData["profile"];
  self: boolean;
  onClaimName?: () => void;
  onEdit?: (field: string) => void;
}) {
  return (
    <header className="ps__head">
      <div className="ps__id">
        <div className="ps__idline">
          <h2 className="ps__name" style={{ color: profile.nameColor }}>
            {profile.name}
            <span className="ps__tag">{profile.tag}</span>
          </h2>
          {self && (
            <button
              className="ps__icon"
              aria-label="Edit name"
              onClick={() => onEdit?.("name")}
            >
              &#x270E;
            </button>
          )}
        </div>
        <div className="ps__addrline">
          <span className="ps__addr">{shortAddr(profile.address)}</span>
          <button
            className="ps__icon ps__icon--sm"
            aria-label="Copy address"
            onClick={() => {
              if (typeof navigator !== "undefined" && navigator.clipboard)
                navigator.clipboard.writeText(profile.address);
            }}
          >
            &#x29C9;
          </button>
        </div>
      </div>
      {self && !profile.hasClaimedName && (
        <button className="ps__claim" onClick={onClaimName}>
          <svg className="ps__claimicon" viewBox="0 0 24 24" aria-hidden="true">
            <path
              fill="currentColor"
              d="M12 1.5l2.36 1.7 2.9-.28 1.13 2.68 2.68 1.13-.28 2.9 1.7 2.37-1.7 2.36.28 2.9-2.68 1.13-1.13 2.68-2.9-.28L12 22.5l-2.36-1.7-2.9.28-1.13-2.68-2.68-1.13.28-2.9L1.5 12l1.7-2.37-.28-2.9 2.68-1.13L6.74 2.92l2.9.28L12 1.5z"
            />
            <path
              fill="none"
              stroke="#ff4d63"
              strokeWidth="2.2"
              strokeLinecap="round"
              strokeLinejoin="round"
              d="M8.5 12.2l2.4 2.4 4.6-4.9"
            />
          </svg>
          CLAIM NAME
        </button>
      )}
    </header>
  );
}

function Tabs({ tab, onTab }: { tab: PassportTab; onTab: (t: PassportTab) => void }) {
  return (
    <div className="tabs tabs--underline" role="tablist" aria-label="Passport sections">
      {TABS.map((t) => (
        <button
          key={t.id}
          type="button"
          role="tab"
          aria-selected={t.id === tab}
          tabIndex={t.id === tab ? 0 : -1}
          className={"tabs__tab" + (t.id === tab ? " is-active" : "")}
          onClick={() => onTab(t.id)}
        >
          {t.label}
        </button>
      ))}
    </div>
  );
}

function Overview({
  data,
  self,
  onLinkClicked,
  onItemClicked,
  onEdit,
}: {
  data: PassportData;
  self: boolean;
  onLinkClicked?: (url: string) => void;
  onItemClicked?: (name: string, category: string) => void;
  onEdit?: (field: string) => void;
}) {
  const { profile, badges } = data;
  return (
    <div className="ps__modules">
      <section className="ps__mod">
        <h3 className="ps__modtitle">Badges</h3>
        <div className="ps__badges">
          {badges.earned.map((b) => (
            <div
              className={"ps__badge ps__badge--" + b.shape}
              key={b.id}
              title={b.tier ? `${b.name} \u{00B7} ${b.tier}` : b.name}
              style={{ ["--t" as string]: b.tint }}
            >
              <span className="ps__badgeart" />
            </div>
          ))}
          {badges.earned.length === 0 &&
            (badges.unavailable ? (
              <p className="ps__failed" role="alert">
                We couldn't load badges: {badges.unavailable}. This is not "no
                badges" &#x2014; nothing was read.
              </p>
            ) : (
              <p className="ps__empty">No badges yet.</p>
            ))}
        </div>
      </section>

      <section className="ps__mod">
        <div className="ps__modhead">
          <h3 className="ps__modtitle">About me</h3>
          {self && (
            <button className="ps__edit" aria-label="Edit" onClick={() => onEdit?.("about")}>
              &#x270E;
            </button>
          )}
        </div>
        {profile.description ? (
          <p className="ps__empty" style={{ color: "rgba(255,255,255,.86)" }}>
            {profile.description}
          </p>
        ) : (
          <p className="ps__empty">No intro.</p>
        )}
        {profile.info.length > 0 && (
          <div className="ps__badges" style={{ gap: 10, marginTop: 12 }}>
            {profile.info.map((f) => (
              <span
                key={f.key}
                className="ps__eqrarity"
                style={{ background: "rgba(255,255,255,.14)", color: "#fff" }}
                title={f.label}
              >
                {f.label}: {f.value}
              </span>
            ))}
          </div>
        )}
      </section>

      <section className="ps__mod">
        <div className="ps__modhead">
          <h3 className="ps__modtitle">Links</h3>
          {self && (
            <button className="ps__edit" aria-label="Edit" onClick={() => onEdit?.("links")}>
              &#x270E;
            </button>
          )}
        </div>
        {profile.links.length > 0 ? (
          <div className="ps__badges" style={{ gap: 10 }}>
            {profile.links.map((l) => (
              <a
                key={l.url}
                href={l.url}
                target="_blank"
                rel="noreferrer noopener"
                className="ps__eqrarity"
                style={{ background: "rgba(255,255,255,.16)", color: "#fff", textDecoration: "none" }}
                onClick={() => onLinkClicked?.(l.url)}
              >
                {l.title}
              </a>
            ))}
          </div>
        ) : (
          <p className="ps__empty">No links.</p>
        )}
      </section>

      <section className="ps__mod">
        <h3 className="ps__modtitle">Equipped items</h3>
        {profile.equipped.length > 0 ? (
          <div className="ps__equipped">
            {profile.equipped.map((it) => (
              <button
                type="button"
                className="ps__eqcard"
                key={it.name}
                style={{ ["--rar" as string]: rarTint(it.rarity), textAlign: "left", cursor: "pointer" }}
                onClick={() => onItemClicked?.(it.name, it.category)}
              >
                <div className="ps__eqart">
                  <span className="ps__eqcat" aria-hidden="true">
                    {it.cat}
                  </span>
                </div>
                <div className="ps__eqname u-truncate" title={it.name}>
                  {it.name}
                </div>
                <span className="ps__eqrarity">{it.rarity}</span>
              </button>
            ))}
          </div>
        ) : (
          <p className="ps__empty">No items equipped.</p>
        )}
      </section>
    </div>
  );
}

const KNOWN_RARITIES = new Set([
  "common",
  "uncommon",
  "rare",
  "epic",
  "legendary",
  "mythic",
  "unique",
  "exotic",
]);
function rarTint(r: string): string {
  const key = r.toLowerCase();
  return rarityColor(KNOWN_RARITIES.has(key) ? key : "common");
}

function Badges({ badges }: { badges: PassportData["badges"] }) {
  const filters = ["All", ...(badges.categories ?? [])];
  const [filter, setFilter] = useState(filters[0]);
  const sections =
    filter === "All"
      ? badges.sections
      : badges.sections.filter((s) => s.label === filter);
  return (
    <div className="ps__modules">
      <div className="bgd__filters" role="tablist" aria-label="Badge category">
        {filters.map((f) => (
          <button
            key={f}
            type="button"
            role="tab"
            aria-selected={f === filter}
            className={"bgd__filter" + (f === filter ? " is-active" : "")}
            onClick={() => setFilter(f)}
          >
            {f}
          </button>
        ))}
      </div>
      {badges.categories === null && (
        <p className="ps__failed">
          The badge category list couldn't be read, so only All is offered.
        </p>
      )}
      {badges.unavailable && (
        <p className="ps__failed" role="alert">
          We couldn't load badges: {badges.unavailable}. Nothing below is a
          count of what this player has earned.
        </p>
      )}
      <div className="bgd__body">
        {sections.map((s) => (
          <section className="bgd__section" key={s.id}>
            <h3 className="bgd__secttitle">{s.label}</h3>
            <div className="bgd__grid">
              {s.badges.map((b) => (
                <div
                  key={b.id}
                  className={"bgd__card" + (b.unlocked ? "" : " is-locked")}
                  style={{ ["--t" as string]: TIER_TINT[b.tier] ?? "#cd7f32" }}
                  title={b.name}
                >
                  {b.isNew && <span className="bgd__new">NEW</span>}
                  <span
                    className={"bgd__art" + (b.unlocked ? "" : " is-locked")}
                    style={{ ["--t" as string]: TIER_TINT[b.tier] ?? "#cd7f32", width: 56, height: 56 }}
                  >
                    <svg viewBox="0 0 64 64" width="56%" height="56%" aria-hidden="true">
                      <path
                        d="M32 5l23.4 13.5v27L32 59 8.6 45.5v-27z"
                        fill="rgba(255,255,255,.18)"
                        stroke="rgba(255,255,255,.55)"
                        strokeWidth="2"
                      />
                      <circle cx="32" cy="30" r="9.5" fill="rgba(255,255,255,.7)" />
                    </svg>
                  </span>
                  <span className="bgd__cardname">{b.name}</span>
                  <span className="bgd__nexttier">{b.unlocked ? b.completedAt ?? "Unlocked" : "NEXT TIER"}</span>
                </div>
              ))}
            </div>
          </section>
        ))}
      </div>
    </div>
  );
}

function Photos({
  photos,
  unavailable,
  onOpenPhoto,
}: {
  photos: PassportPhoto[];
  unavailable: string | null;
  onOpenPhoto: (id: string) => void;
}) {
  if (photos.length === 0 && unavailable) {
    return (
      <div className="ps__modules" style={{ paddingTop: 14 }}>
        <p className="ps__failed" role="alert">
          We couldn't load photos: {unavailable}. This player may well have
          some &#x2014; the camera reel did not answer.
        </p>
      </div>
    );
  }
  return (
    <div className="ps__modules" style={{ paddingTop: 14 }}>
      {photos.length === 0 ? (
        <div className="pp2__empty">
          <div className="pp2__emptyicon" aria-hidden="true">
            <svg viewBox="0 0 24 24" width="34" height="34" fill="currentColor">
              <path d="M4.5 3.5h15A2.5 2.5 0 0 1 22 6v12a2.5 2.5 0 0 1-2.5 2.5h-15A2.5 2.5 0 0 1 2 18V6a2.5 2.5 0 0 1 2.5-2.5Zm0 14.7 4.4-4.4a1 1 0 0 1 1.4 0l2 2 3.6-3.6a1 1 0 0 1 1.4 0l3.3 3.3V6a.5.5 0 0 0-.5-.5H4.5A.5.5 0 0 0 4 6v12c0 .07.01.14.04.2ZM8.5 7.5a2 2 0 1 1 0 4 2 2 0 0 1 0-4Z" />
            </svg>
          </div>
          <div className="pp2__emptytext">There are no photos to show yet</div>
        </div>
      ) : (
        <div
          className="pp2__grid"
          style={{ display: "grid", gridTemplateColumns: "repeat(3, 1fr)", gap: 12 }}
        >
          {photos.map((p) => (
            <button
              type="button"
              key={p.id}
              className="pp2__photo"
              aria-label={`Photo ${p.place?.name ?? ""}`.trim()}
              onClick={() => onOpenPhoto(p.id)}
              style={{
                ["--hue" as string]: String(p.hue),
                aspectRatio: "1",
                border: "none",
                borderRadius: 12,
                cursor: "pointer",
                background: `linear-gradient(150deg, hsl(${p.hue} 70% 60%), hsl(${(p.hue + 40) % 360} 60% 42%))`,
              }}
            />
          ))}
        </div>
      )}
    </div>
  );
}

function PhotoLightbox({
  photo,
  onClose,
}: {
  photo: PassportPhoto;
  onClose: () => void;
}) {
  const date = formatDate(photo.dateTime);
  const cardRef = useRef<HTMLDivElement>(null);
  useDialogKeys(cardRef, onClose);
  return (
    <div className="ep__backdrop" style={{ zIndex: 10 }} onClick={onClose}>
      <div
        className="pd"
        role="dialog"
        aria-modal="true"
        aria-label="Photo"
        tabIndex={-1}
        ref={cardRef}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="pd__photo">
          <div
            className="pd__photoart"
            style={{
              background: `linear-gradient(150deg, hsl(${photo.hue} 70% 55%), hsl(${(photo.hue + 50) % 360} 60% 35%))`,
            }}
          />
          <button className="pd__close" aria-label="Close" onClick={onClose}>
            &#xD7;
          </button>
        </div>

        <aside className="pd__info">
          <div className="pd__meta">{date}</div>

          {photo.place && (
            <section className="pd__sec">
              <div className="pd__seclabel">Place</div>
              <div className="pd__placerow">
                <svg viewBox="0 0 24 24" width="16" height="16" className="pd__pin" aria-hidden="true">
                  <path d="M12 21s7-6.2 7-11a7 7 0 1 0-14 0c0 4.8 7 11 7 11Z" fill="none" stroke="currentColor" strokeWidth="1.8" />
                  <circle cx="12" cy="10" r="2.4" fill="currentColor" />
                </svg>
                <span className="pd__placename u-truncate">
                  {photo.place.name}, {photo.place.x},{photo.place.y}
                </span>
                <button className="pd__go" aria-label="Jump in">
                  &#x2192;
                </button>
              </div>
            </section>
          )}

          <section className="pd__sec">
            <div className="pd__seclabel">People</div>
            {photo.people.length === 0 ? (
              <div className="pd__wearempty">No visible people recorded.</div>
            ) : (
              photo.people.map((p) => (
                <div className="pd__person" key={p.name + p.tag}>
                  <div className="pd__personhead">
                    <span className="pd__pname">
                      {p.name}
                      <span className="pd__ptag">{p.tag}</span>
                    </span>
                  </div>
                  <div className="pd__weartitle">Collectible Wearables</div>
                  {p.wearables.length === 0 ? (
                    <div className="pd__wearempty">
                      No collectibles equipped when the photo was taken
                    </div>
                  ) : (
                    <div className="pd__wearlist">
                      {p.wearables.map((w) => (
                        <div className="pd__wear" key={w}>
                          <span className="pd__wearart" />
                          <div className="pd__wearinfo">
                            <div className="pd__wearname u-truncate">{w}</div>
                          </div>
                          <button className="pd__buy">BUY</button>
                        </div>
                      ))}
                    </div>
                  )}
                </div>
              ))
            )}
          </section>
        </aside>
      </div>
    </div>
  );
}

function formatDate(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleDateString(undefined, { year: "numeric", month: "long", day: "numeric" });
}
