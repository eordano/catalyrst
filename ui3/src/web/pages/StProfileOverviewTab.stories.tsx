import type { Meta, StoryObj } from "@storybook/react-vite";
import StProfileOverviewTab from "./StProfileOverviewTab";
import type { OverviewProfile } from "./StProfileOverviewTab";

const PROFILE: OverviewProfile = {
  address: "0x2fa1b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0",
  name: "PixelNomad",
  hasClaimedName: true,
  nameColor: "#FF8362",
  mutualCount: 3,
  badges: [
    { id: "b1", name: "Open for Business", grad: "linear-gradient(135deg,#ffc95b,#ff743a)" },
    { id: "b2", name: "Land Architect", grad: "linear-gradient(135deg,#b05cff,#438fff)" },
    { id: "b3", name: "Emotionista", grad: "linear-gradient(135deg,#ff4bed,#982de2)" },
    { id: "b4", name: "Wearable Designer", grad: "linear-gradient(135deg,#34ce76,#73d3d3)" },
    { id: "b5", name: "Event Enthusiast", grad: "linear-gradient(135deg,#ff2d55,#ff743a)" },
  ],
  bio: "Builder, collector and occasional DJ. Hanging out in Decentraland since the beginning \u{2014} find me at my plaza most evenings. Always up for a scene jam.",
  info: [
    { key: "country", label: "Country", value: "Argentina", icon: "globe" },
    { key: "language", label: "Language", value: "Spanish, English", icon: "translate" },
    { key: "pronouns", label: "Pronouns", value: "they/them", icon: "pronouns" },
    { key: "relationship_status", label: "Relationship status", value: "In a relationship", icon: "heart" },
    { key: "gender", label: "Gender", value: "Non-binary", icon: "gender" },
    { key: "profession", label: "Profession", value: "Game Designer", icon: "games" },
    { key: "birth_date", label: "Birth date", value: "March 14", icon: "cake" },
    { key: "real_name", label: "Real name", value: "Alex", icon: "at" },
    { key: "favorite_hobby", label: "Favorite hobby", value: "Building scenes", icon: "heart" },
  ],
  links: [
    { title: "Twitter", url: "https://twitter.com/" },
  ],
  equipped: [
    { id: "w1", name: "Cyber Halo", creator: "Neon Dreams", price: "350", rarity: "epic", network: "polygon" },
    { id: "w2", name: "Aurora Jacket", creator: "Polar Series", price: "1,200", rarity: "legendary", network: "polygon" },
    { id: "w3", name: "Glitch Sneakers", creator: "Static Lab", price: "85", rarity: "rare", network: "polygon" },
    { id: "w4", name: "Founders Crown", creator: "DCL Originals", rarity: "mythic", network: "ethereum" },
  ],
};

const NEWCOMER: OverviewProfile = {
  address: "0x9b8a7c6d5e4f3a2b1c0d9e8f7a6b5c4d3e2f1a0b",
  name: "newcomer",
  hasClaimedName: false,
  nameColor: "#73D3D3",
  mutualCount: 0,
  badges: [],
  bio: "",
  info: [],
  links: [],
  equipped: [],
};

/** The profile is picked by name: the fully-populated one, or an empty one in either accent. */
const PROFILES = {
  populated: PROFILE,
  emptyTeal: NEWCOMER,
  emptyAmber: { ...NEWCOMER, nameColor: "#FFC95B" },
};
type ProfileKey = keyof typeof PROFILES;

/** Story args: the profile fixture is picked by name, the rest are real props. */
type OverviewStoryArgs = {
  profilePreset: ProfileKey;
  isOwnProfile: boolean;
  loading: boolean;
};

const BASE: OverviewStoryArgs = { profilePreset: "populated", isOwnProfile: false, loading: false };

function renderTab({
  profilePreset,
  isOwnProfile,
  loading,
  chrome,
  labelSuffix,
}: OverviewStoryArgs & { chrome?: boolean; labelSuffix?: string }) {
  return (
    <StProfileOverviewTab
      profile={PROFILES[profilePreset]}
      isOwnProfile={isOwnProfile}
      loading={loading}
      chrome={chrome}
      labelSuffix={labelSuffix}
    />
  );
}

const meta = {
  title: "Web/Pages/Profile/Overview Tab",
  component: StProfileOverviewTab,
  parameters: { layout: "fullscreen" },
  argTypes: {
    profilePreset: {
      control: "select",
      options: ["populated", "emptyTeal", "emptyAmber"],
      description:
        "Which profile fixture is rendered \u{2014} badges/bio/info/links/equipped filled, or all empty.",
    },
    isOwnProfile: {
      control: "boolean",
      description: "Switches the member tab set for the owner tab set and the owner-only CTAs.",
    },
    loading: { control: "boolean", description: "Renders the skeleton layout." },
  },
  args: BASE,
  render: renderTab,
} satisfies Meta<OverviewStoryArgs>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

const CATALOG: { label: string; args: OverviewStoryArgs }[] = [
  { label: "member view", args: BASE },
  { label: "own profile", args: { ...BASE, isOwnProfile: true } },
  { label: "empty \u{2014} member", args: { profilePreset: "emptyTeal", isOwnProfile: false, loading: false } },
  { label: "empty \u{2014} owner", args: { profilePreset: "emptyAmber", isOwnProfile: true, loading: false } },
  { label: "loading", args: { ...BASE, loading: true } },
];

/**
 * Every state at once. `Default` flips between them from the Controls panel; this keeps the
 * member view, the owner view, both empty profiles and the skeleton in the render + a11y +
 * visual-diff gates. `chrome={false}` so stacking does not emit N `<main>` landmarks, and
 * `labelSuffix` rides through to `ProfileTabLayout`, which owns this page's two named
 * landmarks (`section[aria-label="Profile summary"]` and `nav[aria-label="Profile sections"]`) --
 * axe's `landmark-unique` compares accessible names, so fixed labels would fail N times.
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
          {renderTab({ ...entry.args, chrome: false, labelSuffix: `(${entry.label})` })}
        </section>
      ))}
    </div>
  ),
};
