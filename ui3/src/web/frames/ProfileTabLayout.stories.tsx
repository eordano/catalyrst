import type { Meta, StoryObj } from "@storybook/react-vite";
import type { ComponentProps, CSSProperties, ReactNode } from "react";
import WearablePreview from "../../wearable-preview/WearablePreview";
import ProfileTabLayout from "./ProfileTabLayout";
import AssetCard from "../../marketplace/components/AssetCard";
import CardGrid from "../../components/CardGrid";

type ProfileTabLayoutProps = ComponentProps<typeof ProfileTabLayout>;
type Profile = ProfileTabLayoutProps["profile"];

const MEMBER_TABS = [
  { id: "overview", label: "Overview" },
  { id: "creations", label: "Creations" },
  { id: "communities", label: "Communities" },
  { id: "places", label: "Places" },
  { id: "photos", label: "Photos" },
];

const OWN_TABS = [
  { id: "overview", label: "Overview" },
  { id: "assets", label: "My Assets" },
  { id: "communities", label: "My Communities" },
  { id: "places", label: "My Places" },
  { id: "photos", label: "My Photos" },
  { id: "referral-rewards", label: "Referral Rewards" },
];

const TABS = { member: MEMBER_TABS, own: OWN_TABS };
const TABS_KEYS = Object.keys(TABS);

const SHOWCASE_ADDRESS = "0xf12c21d3edb2c0e68935a3bbe5d68ae4bf9dcd7c";

const PROFILE = {
  address: "0x2fa1b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0",
  name: "PixelNomad",
  nameColor: "#FF8362",
  hasClaimedName: true,
};

const PROFILES = {
  claimed: PROFILE,
  unclaimed: { ...PROFILE, name: "guest-7e8f9a0", nameColor: "#73d3d3", hasClaimedName: false },
  noNameColor: {
    address: "0x742d35cc6634c0532925a3b844bc454e4438f44e",
    name: "metaverse.dcl.eth",
    hasClaimedName: true,
  },
} satisfies Record<string, Profile>;
const PROFILE_KEYS = Object.keys(PROFILES);

const ALL_TAB_IDS = [
  "overview",
  "creations",
  "communities",
  "places",
  "photos",
  "assets",
  "referral-rewards",
];

const btn: CSSProperties = {
  height: 40,
  padding: "0 24px",
  borderRadius: 10,
  border: "none",
  background: "#d80029",
  color: "#fff",
  fontWeight: 600,
  fontSize: 14,
  textTransform: "uppercase",
  cursor: "pointer",
};
const icoBtn: CSSProperties = {
  width: 40,
  height: 40,
  borderRadius: "50%",
  border: "none",
  background: "transparent",
  color: "#fff",
  cursor: "pointer",
};

type OwnedAsset = {
  id: string;
  name: string;
  collection: string;
  price?: string;
  rarity: string;
  network: string;
};

const OWNED_ASSETS: OwnedAsset[] = [
  { id: "a1", name: "Cyber Halo", collection: "Neon Dreams", price: "350", rarity: "epic", network: "polygon" },
  { id: "a2", name: "Aurora Jacket", collection: "Polar Series", price: "1,200", rarity: "legendary", network: "polygon" },
  { id: "a3", name: "Glitch Sneakers", collection: "Static Lab", price: "85", rarity: "rare", network: "polygon" },
  { id: "a4", name: "Founders Crown", collection: "DCL Originals", rarity: "mythic", network: "ethereum" },
];

const MemberActions = () => (
  <>
    <button style={btn}>Add friend</button>
    <button style={icoBtn} aria-label="More actions">&#x22EF;</button>
  </>
);

const Panel = ({ children }: { children?: ReactNode }) => (
  <section
    style={{
      background: "rgba(0,0,0,.2)",
      borderRadius: 16,
      padding: "30px 40px",
      color: "#fcfcfc",
    }}
  >
    {children}
  </section>
);

/** The header action clusters each former variant story passed. */
const ACTIONS = {
  none: undefined,
  member: <MemberActions />,
  manageWorld: <button style={btn}>Manage World</button>,
  addFriend: <button style={btn}>Add friend</button>,
  editProfile: (
    <>
      <button style={btn}>Edit profile</button>
      <button style={icoBtn} aria-label="More actions">&#x22EF;</button>
    </>
  ),
};

/** The split-layout `aside`; `none` collapses the layout back to a single column. */
const ASIDE = {
  none: undefined,
  wearablePreview: (
    <div style={{ width: "100%", height: 480 }}>
      <WearablePreview profile={SHOWCASE_ADDRESS} emote="wave" />
    </div>
  ),
};

/** The tab body each former variant story rendered as `children`. */
const BODY = {
  overviewPanels: (
    <>
      <Panel>
        <h2 style={{ margin: 0, textTransform: "uppercase", fontSize: 16 }}>About</h2>
        <p>Builder, collector and occasional DJ. Hanging out in Decentraland since the beginning.</p>
      </Panel>
      <Panel>
        <h2 style={{ margin: 0, textTransform: "uppercase", fontSize: 16 }}>Equipped items</h2>
      </Panel>
    </>
  ),
  assetsGrid: (
    <>
      <p style={{ marginTop: 0, color: "#fcfcfc" }}>{OWNED_ASSETS.length} assets</p>
      <CardGrid min={220}>
        {OWNED_ASSETS.map((a) => (
          <AssetCard
            key={a.id}
            name={a.name}
            collection={a.collection}
            price={a.price}
            rarity={a.rarity}
            network={a.network}
          />
        ))}
      </CardGrid>
    </>
  ),
  nothingCreated: (
    <div style={{ color: "rgba(255,255,255,.45)", fontStyle: "italic" }}>
      This account has not created anything yet.
    </div>
  ),
  assetCount: <p style={{ marginTop: 0, color: "#fcfcfc" }}>8 assets</p>,
  photoCount: <p style={{ marginTop: 0, color: "#fcfcfc" }}>12 photos</p>,
  referralHero: (
    <div style={{ color: "#fcfcfc", textAlign: "center" }}>
      <h1 style={{ fontSize: 36 }}>Decentraland is Better With Friends</h1>
      <p>Invite yours and get rewards!</p>
    </div>
  ),
};

const meta = {
  title: "Web/Frames/ProfileTabLayout",
  component: ProfileTabLayout,
  parameters: { layout: "fullscreen" },
  argTypes: {
    // `profile` and `tabs` are object-valued props, so their presets ride the same
    // `options` + `mapping` route `EmptyState` uses for ReactNode props: the control offers
    // the preset names, the mapping resolves each to the real descriptor.
    profile: {
      control: "select",
      options: PROFILE_KEYS,
      mapping: PROFILES,
      description: "Which profile descriptor drives the header identity block.",
    },
    tabs: {
      control: "inline-radio",
      options: TABS_KEYS,
      mapping: TABS,
      description: "`member` is the other-account tab set, `own` the self-view one.",
    },
    activeTab: { control: "select", options: ALL_TAB_IDS },
    actions: { control: "select", options: Object.keys(ACTIONS), mapping: ACTIONS },
    aside: { control: "select", options: Object.keys(ASIDE), mapping: ASIDE },
    children: { control: "select", options: Object.keys(BODY), mapping: BODY },
    showWalletIcon: { control: "boolean" },
    showCopy: { control: "boolean" },
    bodyPadded: { control: "boolean" },
    pageBackground: { control: "text" },
  },
  args: {
    profile: PROFILES.claimed,
    tabs: TABS.member,
    activeTab: "overview",
    actions: "member",
    aside: "wearablePreview",
    children: "overviewPanels",
    showWalletIcon: true,
    showCopy: true,
    bodyPadded: false,
  },
} satisfies Meta<typeof ProfileTabLayout>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

const CATALOG: { label: string; props: ProfileTabLayoutProps }[] = [
  {
    label: "overview \u{2014} split with aside",
    props: {
      profile: PROFILES.claimed,
      tabs: TABS.member,
      activeTab: "overview",
      actions: ACTIONS.member,
      aside: ASIDE.wearablePreview,
      children: BODY.overviewPanels,
    },
  },
  {
    label: "full width \u{2014} owner asset grid",
    props: {
      profile: PROFILES.claimed,
      tabs: TABS.own,
      activeTab: "assets",
      actions: ACTIONS.manageWorld,
      aside: ASIDE.none,
      children: BODY.assetsGrid,
      bodyPadded: true,
    },
  },
  {
    label: "unclaimed name \u{2014} #abcd discriminator",
    props: {
      profile: PROFILES.unclaimed,
      tabs: TABS.member,
      activeTab: "creations",
      actions: ACTIONS.member,
      aside: ASIDE.none,
      children: BODY.nothingCreated,
      bodyPadded: true,
    },
  },
  {
    label: "assets header \u{2014} no wallet icon, no copy",
    props: {
      profile: PROFILES.claimed,
      tabs: TABS.own,
      activeTab: "assets",
      actions: ACTIONS.none,
      aside: ASIDE.none,
      children: BODY.assetCount,
      showWalletIcon: false,
      showCopy: false,
      bodyPadded: true,
    },
  },
  {
    label: "no nameColor \u{2014} hue derived from the name",
    props: {
      profile: PROFILES.noNameColor,
      tabs: TABS.member,
      activeTab: "photos",
      actions: ACTIONS.addFriend,
      aside: ASIDE.none,
      children: BODY.photoCount,
      bodyPadded: true,
    },
  },
  {
    label: "referral rewards tab active",
    props: {
      profile: PROFILES.claimed,
      tabs: TABS.own,
      activeTab: "referral-rewards",
      actions: ACTIONS.editProfile,
      aside: ASIDE.none,
      children: BODY.referralHero,
      bodyPadded: true,
    },
  },
];

/**
 * Every state at once. `Default` flips between them from the Controls panel -- every prop the
 * former variant stories differed by (`profile`, `tabs`, `activeTab`, `actions`, `aside`,
 * `children`, `showWalletIcon`, `showCopy`, `bodyPadded`) is an `options` + `mapping` control.
 * This keeps the split layout, the full-width body, both header variants and the two profile
 * fallbacks in the render + a11y + visual-diff gates.
 *
 * Each entry gets a `labelSuffix` because the frame owns two named landmarks --
 * `section[aria-label="Profile summary"]` and `nav[aria-label="Profile sections"]`. axe's
 * `landmark-unique` compares accessible names, so fixed labels would fail six times over. The
 * bare `<section>` wrapper demotes the split layout's unnamed `<aside>` to `generic` under the
 * HTML-AAM scoped mapping. The frame has no `chrome` prop and emits no `<main>`, so none is needed.
 */
export const Catalog: Story = {
  name: "Catalog (every state)",
  parameters: { controls: { disable: true } },
  render: () => (
    <div style={{ display: "flex", flexDirection: "column", gap: 32 }}>
      {CATALOG.map((entry) => (
        <section key={entry.label}>
          <div style={{ font: "600 13px var(--font-sans)", opacity: 0.7, margin: "0 0 8px" }}>
            {entry.label}
          </div>
          <ProfileTabLayout {...entry.props} labelSuffix={`(${entry.label})`} />
        </section>
      ))}
    </div>
  ),
};
