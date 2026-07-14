import type { ComponentProps, FC, ReactNode } from "react";
import { SitesChromeMaybe } from "../frames/SitesChrome";
import AssetCardImpl from "../../marketplace/components/AssetCard";
import ProfileTabLayoutImpl from "../frames/ProfileTabLayout";
import type { LabelSuffixProps } from "../../components/labelSuffix";
import "./stprofileoverviewtab.css";

const C = "stprofileoverviewtab";

const AssetCard = AssetCardImpl as FC<Partial<ComponentProps<typeof AssetCardImpl>>>;
const ProfileTabLayout = ProfileTabLayoutImpl as FC<
  Partial<Omit<ComponentProps<typeof ProfileTabLayoutImpl>, "tabs">> & {
    tabs?: TabItem[];
  }
>;

type BadgeItem = { id: string; name: string; grad: string };
type InfoItem = { key: string; label: string; value: string; icon: string };
type LinkItem = { title: string; url: string };
type EquippedItem = {
  id: string;
  name: string;
  creator: string;
  price?: string;
  rarity: string;
  network: string;
  marketplaceUrl?: string;
};

export type OverviewProfile = {
  address: string;
  name?: string;
  hasClaimedName?: boolean;
  nameColor?: string;
  mutualCount: number;
  badges?: BadgeItem[];
  bio?: string;
  info?: InfoItem[];
  links?: LinkItem[];
  equipped?: EquippedItem[];
};

type TabItem = { id: string; label: string };

const EMPTY_PROFILE: OverviewProfile = { address: "", mutualCount: 0 };

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
const EditGlyph = () => (
  <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true">
    <path d="M4 16.5 14.6 5.9a2 2 0 0 1 2.8 0l.7.7a2 2 0 0 1 0 2.8L7.5 20H4v-3.5Z" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinejoin="round" />
  </svg>
);
const GlobeBtnGlyph = () => (
  <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
    <circle cx="12" cy="12" r="9" fill="none" stroke="currentColor" strokeWidth="1.7" />
    <path d="M3 12h18M12 3c2.6 2.6 2.6 15.4 0 18M12 3c-2.6 2.6-2.6 15.4 0 18" fill="none" stroke="currentColor" strokeWidth="1.7" />
  </svg>
);
const LinkGlyph = () => (
  <svg viewBox="0 0 24 24" width="22" height="22" aria-hidden="true">
    <path d="M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1.5 1.5M14 10a4 4 0 0 0-5.7 0l-3 3A4 4 0 0 0 11 18.7l1.5-1.5" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" />
  </svg>
);
const INFO_GLYPHS: Record<string, ReactNode> = {
  globe: (
    <svg viewBox="0 0 20 20" width="20" height="20" aria-hidden="true">
      <circle cx="10" cy="10" r="7.2" fill="none" stroke="currentColor" strokeWidth="1.4" />
      <path d="M2.8 10h14.4M10 2.8c2 2 2 12.4 0 14.4M10 2.8c-2 2-2 12.4 0 14.4" fill="none" stroke="currentColor" strokeWidth="1.4" />
    </svg>
  ),
  translate: (
    <svg viewBox="0 0 20 20" width="20" height="20" aria-hidden="true">
      <path d="M3 5h7M6.5 3v2M8.5 5c0 4-3 6.5-5.5 7.5M5 8c.5 2 2.5 3.5 4.5 4.2M11 17l3.2-8 3.2 8M12.2 14h4" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  ),
  pronouns: (
    <svg viewBox="0 0 20 20" width="20" height="20" aria-hidden="true">
      <circle cx="8.4" cy="7.5" r="4.3" fill="none" stroke="currentColor" strokeWidth="1.4" />
      <circle cx="5.4" cy="12.5" r="4.3" fill="none" stroke="currentColor" strokeWidth="1.4" />
      <circle cx="11.5" cy="12.5" r="4.3" fill="none" stroke="currentColor" strokeWidth="1.4" />
    </svg>
  ),
  gender: (
    <svg viewBox="0 0 20 20" width="20" height="20" aria-hidden="true">
      <circle cx="10" cy="9" r="4.2" fill="none" stroke="currentColor" strokeWidth="1.4" />
      <path d="M10 13.2V18M7.5 16h5M13.5 5.5 16.5 2.5M14 2.5h2.5V5" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
    </svg>
  ),
  games: (
    <svg viewBox="0 0 20 20" width="20" height="20" aria-hidden="true">
      <rect x="2" y="6.5" width="16" height="8.5" rx="4.2" fill="none" stroke="currentColor" strokeWidth="1.4" />
      <path d="M5.5 9.5v3M4 11h3M13 10h.01M15.5 11.5h.01" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
    </svg>
  ),
  heart: (
    <svg viewBox="0 0 20 20" width="20" height="20" aria-hidden="true">
      <path d="M10 16.5S3 12.5 3 7.8A3.6 3.6 0 0 1 10 6a3.6 3.6 0 0 1 7 1.8c0 4.7-7 8.7-7 8.7Z" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinejoin="round" />
    </svg>
  ),
  at: (
    <svg viewBox="0 0 20 20" width="20" height="20" aria-hidden="true">
      <circle cx="10" cy="10" r="3.3" fill="none" stroke="currentColor" strokeWidth="1.4" />
      <path d="M13.3 10v1.6a2.1 2.1 0 0 0 4.2 0V10a7.5 7.5 0 1 0-3 6" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
    </svg>
  ),
  cake: (
    <svg viewBox="0 0 20 20" width="20" height="20" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M3.5 17.5h13M4.5 17.5v-5a2 2 0 0 1 2-2h7a2 2 0 0 1 2 2v5" />
      <path d="M4.5 13.2c1.05 0 1.05.9 2.1.9s1.05-.9 2.1-.9 1.05.9 2.1.9 1.05-.9 2.1-.9 1.05.9 2.1.9" />
      <path d="M10 7.5V5" />
      <circle cx="10" cy="3.6" r="0.9" fill="currentColor" stroke="none" />
    </svg>
  ),
};

function EditProfileButton({ floating }: { floating?: boolean }) {
  return (
    <button type="button" className={`${C}__edit` + (floating ? ` ${C}__edit--float` : "")}>
      <EditGlyph />
      Edit profile
    </button>
  );
}

type StProfileOverviewTabProps = LabelSuffixProps & {
  chrome?: boolean;
  profile?: OverviewProfile;
  activeTab?: string;
  isOwnProfile?: boolean;
  loading?: boolean;
};

export default function StProfileOverviewTab({
  labelSuffix,
  chrome = true,
  profile = EMPTY_PROFILE,
  activeTab = "overview",
  isOwnProfile = false,
  loading = false,
}: StProfileOverviewTabProps) {
  const initial = (profile.name || profile.address || "?").charAt(0).toUpperCase();
  const tabs = isOwnProfile ? OWN_TABS : MEMBER_TABS;
  const hasClaimedName = profile.hasClaimedName ?? false;

  return (
    <SitesChromeMaybe chrome={chrome} active="legal" overlayNav>
      <ProfileTabLayout
        className={C}
        labelSuffix={labelSuffix}
        profile={profile}
        tabs={tabs}
        activeTab={activeTab}
        actions={
          isOwnProfile ? (
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
              <button type="button" className={`${C}__cta`}>
                <PersonAddGlyph />
                Add friend
              </button>
              <button type="button" className={`${C}__more`} aria-label="More actions">
                <MoreGlyph />
              </button>
            </>
          )
        }
        aside={
          <div className={`${C}__render`} role="img" aria-label="Avatar preview">
            <span className={`${C}__renderglow`} aria-hidden="true" />
            <span className={`${C}__renderfig`} style={{ background: profile.nameColor }} aria-hidden="true">
              {initial}
            </span>
          </div>
        }
      >
        {loading ? (
          <div className={`${C}__surface ${C}__loadingrow`}>
            <span className={`${C}__spinner`} role="status" aria-label="Loading profile" />
          </div>
        ) : (
          <>
            <section className={`${C}__surface`}>
              {isOwnProfile ? <EditProfileButton floating /> : null}

              <div className={`${C}__section`}>
                <h2 className={`${C}__sectitle`}>Badges</h2>
                {(profile.badges?.length ?? 0) > 0 ? (
                  <div className={`${C}__badges`}>
                    {profile.badges?.map((b) => (
                      <span
                        key={b.id}
                        className={`${C}__badge`}
                        style={{ background: b.grad }}
                        title={b.name}
                        tabIndex={0}
                        aria-label={b.name}
                      >
                        <span className={`${C}__badgeinit`}>{b.name.charAt(0)}</span>
                      </span>
                    ))}
                  </div>
                ) : (
                  <p className={`${C}__empty`}>No badges yet. Jump in world and start collecting badges!</p>
                )}
              </div>

              <div className={`${C}__section`}>
                <h2 className={`${C}__sectitle`}>About</h2>
                {profile.bio ? (
                  <p className={`${C}__bio`}>{profile.bio}</p>
                ) : (
                  <p className={`${C}__empty`}>
                    {isOwnProfile
                      ? "No information yet. Jump in world and complete your bio!"
                      : "This member has not added a bio yet."}
                  </p>
                )}
                {(profile.info?.length ?? 0) > 0 ? (
                  <div className={`${C}__infogrid`}>
                    {profile.info?.map((f) => (
                      <div key={f.key} className={`${C}__infoitem`}>
                        <span className={`${C}__infolabel`}>
                          <span className={`${C}__infoicon`}>{INFO_GLYPHS[f.icon]}</span>
                          {f.label}
                        </span>
                        <span className={`${C}__infovalue`}>{f.value}</span>
                      </div>
                    ))}
                  </div>
                ) : null}
              </div>

              {(profile.links?.length ?? 0) > 0 ? (
                <div className={`${C}__section`}>
                  <h2 className={`${C}__sectitle`}>Links</h2>
                  <div className={`${C}__links`}>
                    {profile.links?.map((l) => (
                      <a
                        key={l.title}
                        className={`${C}__linkpill`}
                        href={l.url}
                        target="_blank"
                        rel="noopener noreferrer"
                      >
                        <span className={`${C}__linkicon`}><LinkGlyph /></span>
                        {l.title}
                      </a>
                    ))}
                  </div>
                </div>
              ) : null}
            </section>

            <section className={`${C}__surface`}>
              <div className={`${C}__section`}>
                <h2 className={`${C}__sectitle`}>Equipped items</h2>
                {(profile.equipped?.length ?? 0) > 0 ? (
                  <div className={`${C}__equipped`}>
                    {profile.equipped?.map((item) => (
                      <div key={item.id} className={`${C}__equippeditem`}>
                        <AssetCard
                          name={item.name}
                          collection={`By ${item.creator}`}
                          price={item.price}
                          rarity={item.rarity}
                          network={item.network}
                        />
                        {!isOwnProfile ? (
                          <a
                            className={`${C}__buy`}
                            href={item.marketplaceUrl || "#"}
                            target="_blank"
                            rel="noopener noreferrer"
                          >
                            Buy
                          </a>
                        ) : null}
                      </div>
                    ))}
                  </div>
                ) : (
                  <p className={`${C}__empty`}>No equipped items yet.</p>
                )}
              </div>
            </section>
          </>
        )}
      </ProfileTabLayout>
    </SitesChromeMaybe>
  );
}
