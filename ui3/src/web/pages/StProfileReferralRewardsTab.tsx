import { siteUrl } from "../../data/site";
import type { CSSProperties } from "react";
import { useState } from "react";
import { SitesChromeMaybe } from "../frames/SitesChrome";
import { asset } from "../../asset";
import { suffixLabel, type LabelSuffixProps } from "../../components/labelSuffix";
import "./stprofilereferralrewardstab.css";

const C = "strrt";

const RARITY_COLOR: Record<string, string> = {
  unique: "#FFB626",
  mythic: "#FF63E1",
  exotic: "#CAFF73",
  legendary: "#842DDA",
  epic: "#3D85E6",
  rare: "#36CF75",
  uncommon: "#ED6D4F",
  common: "#ABC1C1",
};
const RARITY_TEXT_DARK: Record<string, boolean> = {
  exotic: true,
  mythic: true,
  epic: true,
  rare: true,
  uncommon: true,
  common: true,
  unique: true,
};

type Tier = { invitesAccepted: number; rarity: string; description: string };

const TIERS: Tier[] = [
  { invitesAccepted: 5, rarity: "epic", description: "RainbowWave Jeans + Community Recruiter Starter Badge" },
  { invitesAccepted: 10, rarity: "epic", description: "Good Vibes Letterman + Community Recruiter Bronze Badge" },
  { invitesAccepted: 20, rarity: "legendary", description: "Signal Surge Gloves" },
  { invitesAccepted: 25, rarity: "legendary", description: "Postman Emote + Community Recruiter Silver Badge" },
  { invitesAccepted: 30, rarity: "exotic", description: "Afterglow Kicks" },
  { invitesAccepted: 50, rarity: "exotic", description: "Spunky MopTop + Community Recruiter Gold Badge" },
  { invitesAccepted: 60, rarity: "mythic", description: "Volty Vibes Shoulder Companion" },
  { invitesAccepted: 75, rarity: "mythic", description: "Monocycle Emote + Community Recruiter Platinum Badge" },
  { invitesAccepted: 100, rarity: "SWAG", description: "IRL Swag Pack + In-World Wearable + Community Recruiter Diamond Badge" },
];

const SEGMENT_PERCENTAGE = 11.1;
const OFFSET = 5;
function calculateProgressPercentage(totalSteps: number, invitedUsersAccepted: number) {
  if (totalSteps <= 0) return 0;
  const firstTier = TIERS[0];
  const lastTier = TIERS[TIERS.length - 1];
  if (invitedUsersAccepted <= (firstTier?.invitesAccepted ?? 0)) return invitedUsersAccepted;
  if (invitedUsersAccepted >= (lastTier?.invitesAccepted ?? 0)) return totalSteps * SEGMENT_PERCENTAGE;
  let prevTierIndex = 0;
  for (let i = 0; i < TIERS.length; i++) {
    const tier = TIERS[i];
    if (tier && invitedUsersAccepted < tier.invitesAccepted) {
      prevTierIndex = i - 1;
      break;
    }
  }
  const prevTier = TIERS[prevTierIndex];
  const nextTier = TIERS[prevTierIndex + 1];
  const invitesNeeded = (nextTier?.invitesAccepted ?? 0) - (prevTier?.invitesAccepted ?? 0);
  const invitesProgress = invitedUsersAccepted - (prevTier?.invitesAccepted ?? 0);
  const progressPercentage = (invitesProgress / invitesNeeded) * SEGMENT_PERCENTAGE;
  const basePercentage = prevTierIndex * SEGMENT_PERCENTAGE + OFFSET;
  return basePercentage + progressPercentage;
}

export type ReferralProfile = {
  address: string;
  name?: string;
  hasClaimedName?: boolean;
  nameColor?: string;
};

export type ReferralData = {
  invitedUsersAccepted: number;
  invitedUsersAcceptedViewed: number;
  rewardImages: string[];
};

type TabItem = { id: string; label: string };

const EMPTY_PROFILE: ReferralProfile = { address: "" };

const TABS: TabItem[] = [
  { id: "overview", label: "Overview" },
  { id: "creations", label: "Creations" },
  { id: "communities", label: "Communities" },
  { id: "places", label: "Places" },
  { id: "photos", label: "Photos" },
  { id: "referral-rewards", label: "Referral Rewards" },
];

function truncateAddress(addr: string) {
  return `${addr.slice(0, 6)}\u{2026}${addr.slice(-4)}`;
}

const WalletGlyph = () => (
  <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
    <path d="M3 7a2 2 0 0 1 2-2h12a2 2 0 0 1 2 2v1h1a1 1 0 0 1 1 1v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7Zm14 7a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3Z" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinejoin="round" />
  </svg>
);
const CopyGlyph = () => (
  <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true">
    <rect x="9" y="9" width="11" height="11" rx="2" fill="none" stroke="currentColor" strokeWidth="1.6" />
    <path d="M5 15V5a2 2 0 0 1 2-2h8" fill="none" stroke="currentColor" strokeWidth="1.6" />
  </svg>
);
const VerifiedGlyph = () => (
  <svg viewBox="0 0 24 24" width="11" height="11" aria-hidden="true">
    <path d="m6 12 4 4 8-9" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round" />
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
  <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
    <path d="M4 16.5V20h3.5L18 9.5 14.5 6 4 16.5ZM20.7 6.3a1 1 0 0 0 0-1.4l-1.6-1.6a1 1 0 0 0-1.4 0L16 5l3.5 3.5 1.2-1.2Z" fill="currentColor" />
  </svg>
);
const InfoGlyph = () => (
  <svg viewBox="0 0 24 24" width="22" height="22" aria-hidden="true">
    <circle cx="12" cy="12" r="9" fill="none" stroke="currentColor" strokeWidth="1.6" />
    <path d="M12 11v5" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" />
    <circle cx="12" cy="7.6" r="1.1" fill="currentColor" />
  </svg>
);
const CopyAdornGlyph = () => (
  <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
    <rect x="9" y="9" width="11" height="11" rx="2" fill="none" stroke="currentColor" strokeWidth="1.7" />
    <path d="M5 15V5a2 2 0 0 1 2-2h8" fill="none" stroke="currentColor" strokeWidth="1.7" />
  </svg>
);
const ShareGlyph = () => (
  <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
    <circle cx="18" cy="5" r="2.4" fill="none" stroke="currentColor" strokeWidth="1.7" />
    <circle cx="6" cy="12" r="2.4" fill="none" stroke="currentColor" strokeWidth="1.7" />
    <circle cx="18" cy="19" r="2.4" fill="none" stroke="currentColor" strokeWidth="1.7" />
    <path d="m8.1 10.9 7.8-4.6M8.1 13.1l7.8 4.6" fill="none" stroke="currentColor" strokeWidth="1.7" />
  </svg>
);
const ChevronGlyph = ({ up }: { up: boolean }) => (
  <svg viewBox="0 0 24 24" width="22" height="22" aria-hidden="true" style={{ transform: up ? "rotate(180deg)" : "none" }}>
    <path d="m7 10 5 5 5-5" fill="none" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" />
  </svg>
);
const CheckRound = () => (
  <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
    <path d="m6 12 4 4 8-9" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round" />
  </svg>
);
const CheckSmall = () => (
  <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true">
    <path d="m6 12 4 4 8-9" fill="none" stroke="currentColor" strokeWidth="2.6" strokeLinecap="round" strokeLinejoin="round" />
  </svg>
);
const LockSmall = () => (
  <svg viewBox="0 0 24 24" width="15" height="15" aria-hidden="true">
    <rect x="5" y="10.5" width="14" height="9.5" rx="2" fill="currentColor" />
    <path d="M8 10.5V8a4 4 0 0 1 8 0v2.5" fill="none" stroke="currentColor" strokeWidth="1.8" />
  </svg>
);

const STEP_GLYPHS = ["\u{2709}\u{FE0F}", "\u{1F4CD}", "\u{1F3C5}"];

type ReferralRewardCardProps = {
  invitesAccepted: number;
  rarity: string;
  completed: boolean;
};

function ReferralRewardCard({ invitesAccepted, rarity, completed }: ReferralRewardCardProps) {
  const isSwag = rarity === "SWAG";
  const color = isSwag ? undefined : RARITY_COLOR[rarity];
  const rarityLabel = isSwag ? "IRL SWAG PACK" : rarity.toUpperCase();
  const darkRarity = RARITY_TEXT_DARK[rarity];

  let cardStyle: CSSProperties;
  if (isSwag && completed) {
    cardStyle = { backgroundImage: "linear-gradient(116.34deg, #FFBC5B 0%, #FF2D55 50.52%, #C640CD 100%)" };
  } else if (isSwag) {
    cardStyle = { backgroundImage: "linear-gradient(116.34deg, rgba(255,188,91,0.4) 0%, rgba(255,45,85,0.4) 50.52%, rgba(198,64,205,0.4) 100%)" };
  } else if (completed) {
    cardStyle = { backgroundColor: color };
  } else {
    cardStyle = { backgroundColor: `${color}40` };
  }

  const artRarity = isSwag ? "unique" : rarity;

  return (
    <div className={`${C}__rwcard`}>
      <div className={`${C}__rwhead`}>
        <span className={`${C}__rwcount`}>{invitesAccepted}</span>
      </div>
      <div className={`${C}__rwborder${completed ? " is-completed" : ""}`}>
        <div className={`${C}__rwbody`} style={cardStyle}>
          <div className={`${C}__rwbar`}>
            <span className={`${C}__rwrarity${darkRarity ? " is-dark" : ""}`}>{rarityLabel}</span>
            <span className={`${C}__rwicon`}>{completed ? <CheckSmall /> : <LockSmall />}</span>
          </div>
          <div className={`${C}__rwimgwrap`}>
            <img
              className={`${C}__rwimg${completed ? "" : " is-locked"}`}
              src={asset(`assets/rarity/${artRarity}.png`)}
              alt="Reward Image"
              loading="lazy"
            />
          </div>
        </div>
      </div>
    </div>
  );
}

function ReferralHeroSection({ profile }: { profile: ReferralProfile }) {
  const [copied, setCopied] = useState(false);
  const [showSteps, setShowSteps] = useState(false);

  const handle =
    profile.hasClaimedName && profile.name
      ? profile.name
      : profile.address
        ? truncateAddress(profile.address)
        : "";
  const inviteLink = `https://decentraland.org/invite/${handle}`;

  const onCopy = () => {
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const STEPS = [
    "Invite friends to hang out in-world by sharing your referral link.",
    "Your friends download Decentraland and jump in on 3 separate days.",
    "You earn exclusive rewards for helping grow our community!",
  ];

  return (
    <section className={`${C}__hero`}>
      <div className={`${C}__envwrap`}>
        <span className={`${C}__envglow`} aria-hidden="true" />
        <span className={`${C}__env`} aria-hidden="true">&#x2709;&#xFE0F;</span>
      </div>

      <div className={`${C}__herowrap`}>
        <h1 className={`${C}__title`}>Decentraland is Better With Friends</h1>
        <p className={`${C}__subtitle`}>
          Invite yours and get rewards!
          <span className={`${C}__infowrap`}>
            <button type="button" className={`${C}__info`} aria-label="More info">
              <InfoGlyph />
            </button>
            <span className={`${C}__tooltip`} role="tooltip">
              Invitees must be new to Decentraland and must jump in-world on 3 separate days to count toward reward goals. See{" "}
              <a className={`${C}__tooltiplink`} href={siteUrl("/referral-terms")} target="_blank" rel="noopener noreferrer">
                Terms &amp; Conditions
              </a>
              .
            </span>
          </span>
        </p>

        <div className={`${C}__invite`}>
          <div className={`${C}__inputrow`}>
            <div className={`${C}__inputfield`} onClick={onCopy}>
              <input className={`${C}__input`} value={inviteLink} readOnly tabIndex={-1} aria-label="Your invite link" />
              <span className={`${C}__inputcopy`} aria-hidden="true">
                <CopyAdornGlyph />
              </span>
              {copied ? <span className={`${C}__copied`}>Link copied successfully</span> : null}
            </div>
            <button type="button" className={`${C}__share`}>
              SHARE
              <ShareGlyph />
            </button>
          </div>
        </div>

        <button type="button" className={`${C}__how`} onClick={() => setShowSteps((p) => !p)} aria-expanded={showSteps}>
          How it works
          <ChevronGlyph up={showSteps} />
        </button>

        <div className={`${C}__steps${showSteps ? " is-open" : ""}`}>
          {STEPS.map((text, i) => (
            <div key={i} className={`${C}__step`}>
              <div className={`${C}__steptext`}>
                <span className={`${C}__stepnum`}>{i + 1}</span>
                <span className={`${C}__stepbody`}>{text}</span>
              </div>
              <span className={`${C}__stepimg`} aria-hidden="true">{STEP_GLYPHS[i]}</span>
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}

function ReferralJourney({
  invitedUsersAccepted,
  labelSuffix,
}: LabelSuffixProps & { invitedUsersAccepted: number }) {
  const totalSteps = TIERS.length;
  const percent = calculateProgressPercentage(totalSteps, invitedUsersAccepted);

  return (
    <section className={`${C}__journey`}>
      <div className={`${C}__jtitlerow`}>
        <h2 className={`${C}__jtitle`}>Your Reward Journey</h2>
        <div className={`${C}__jsubwrap`}>
          <span className={`${C}__jsub`}>&#x1F90D; {invitedUsersAccepted} Friends Joined</span>
        </div>
      </div>

      <div
        className={`${C}__jscroll`}
        role="region"
        aria-label={suffixLabel("Your Reward Journey", labelSuffix)}
        tabIndex={0}
      >
        <div className={`${C}__jwrap`}>
          <div className={`${C}__jstepper`}>
            <div className={`${C}__jline`}>
              <span className={`${C}__jfill`} style={{ width: `${percent}%` }} aria-hidden="true" />
            </div>
            {TIERS.map((tier, i) => {
              const completed = tier.invitesAccepted <= invitedUsersAccepted;
              return (
                <div key={i} className={`${C}__jstep`}>
                  <div className={`${C}__jdot${completed ? " is-completed" : ""}`}>
                    <span className={`${C}__jdotinner`}>{completed ? <CheckRound /> : null}</span>
                  </div>
                  <ReferralRewardCard
                    invitesAccepted={tier.invitesAccepted}
                    rarity={tier.rarity}
                    completed={completed}
                  />
                </div>
              );
            })}
          </div>
        </div>
      </div>
    </section>
  );
}

type StProfileReferralRewardsTabProps = LabelSuffixProps & {
  chrome?: boolean;
  profile?: ReferralProfile;
  tabs?: TabItem[];
  activeTab?: string;
  data?: ReferralData | null;
  state?: "ready" | "loading" | "error";
};

export default function StProfileReferralRewardsTab({
  labelSuffix,
  chrome = true,
  profile = EMPTY_PROFILE,
  tabs = TABS,
  activeTab = "referral-rewards",
  data = null,
  state = "ready",
}: StProfileReferralRewardsTabProps) {
  const initial = (profile.name || profile.address).charAt(0).toUpperCase();

  return (
    <SitesChromeMaybe chrome={chrome} active="legal" overlayNav>
      <div className={C}>
        <div className={`${C}__content`}>
          <div className={`${C}__card`}>
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
                    {profile.hasClaimedName ? (
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
                      <button type="button" className={`${C}__copybtn`} aria-label="Copy address">
                        <CopyGlyph />
                      </button>
                    </div>
                  ) : null}
                </div>
              </div>

              <div className={`${C}__actions`}>
                <button type="button" className={`${C}__cta`}>
                  <EditGlyph />
                  Edit profile
                </button>
                <button type="button" className={`${C}__more`} aria-label="More actions">
                  <MoreGlyph />
                </button>
              </div>
            </div>

            <nav className={`${C}__tabs`} aria-label={suffixLabel("Profile sections", labelSuffix)}>
              {tabs.map((tab) => (
                <button
                  key={tab.id}
                  type="button"
                  className={`${C}__tab` + (tab.id === activeTab ? " is-active" : "")}
                  aria-current={tab.id === activeTab ? "page" : undefined}
                >
                  {tab.label}
                  {tab.id === activeTab ? <span className={`${C}__tabbar`} aria-hidden="true" /> : null}
                </button>
              ))}
            </nav>

            <div className={`${C}__referrals`}>
              {state === "loading" ? (
                <div className={`${C}__statebox`}>
                  <span className={`${C}__spinner`} role="status" aria-label="Loading" />
                </div>
              ) : state === "error" || !data ? (
                <div className={`${C}__statebox`}>
                  <p className={`${C}__anon`}>Sign in to invite friends and unlock rewards.</p>
                </div>
              ) : (
                <>
                  <ReferralHeroSection profile={profile} />
                  <ReferralJourney
                    invitedUsersAccepted={data.invitedUsersAccepted}
                    labelSuffix={labelSuffix}
                  />
                </>
              )}
            </div>
          </div>
        </div>
      </div>
    </SitesChromeMaybe>
  );
}
