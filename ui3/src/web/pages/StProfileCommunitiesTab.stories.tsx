import type { Meta, StoryObj } from "@storybook/react-vite";
import StProfileCommunitiesTab from "./StProfileCommunitiesTab";
import type { Community, Profile } from "./StProfileCommunitiesTab";

const PROFILE: Profile = {
  address: "0x2fa1b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0",
  name: "PixelNomad",
  hasClaimedName: true,
  nameColor: "#FF8362",
  mutualCount: 3,
};

const COMMUNITIES: Community[] = [
  { id: "c1", name: "Neon District Builders", membersCount: 1284, role: "owner", thumb: "linear-gradient(135deg,#ff743a,#ff2d55)" },
  { id: "c2", name: "DCL Photographers Guild", membersCount: 642, role: "admin", thumb: "linear-gradient(135deg,#b05cff,#438fff)" },
  { id: "c3", name: "Wearable Designers Collective", membersCount: 3120, role: "member", thumb: "linear-gradient(135deg,#34ce76,#73d3d3)" },
  { id: "c4", name: "Genesis Plaza Regulars", membersCount: 87, role: "member", thumb: "linear-gradient(135deg,#ff4bed,#982de2)" },
  { id: "c5", name: "Event Hosts United", membersCount: 415, role: "member", thumb: "linear-gradient(135deg,#ffc95b,#ff743a)" },
  { id: "c6", name: "Music Lovers of Decentraland", membersCount: 2056, role: "member", thumb: null },
  { id: "c7", name: "Scene Jam Collective", membersCount: 198, role: "member", thumb: "linear-gradient(135deg,#57c2ff,#7434b1)" },
  { id: "c8", name: "Land Architects", membersCount: 53, role: "member", thumb: "linear-gradient(135deg,#73d3d3,#438fff)" },
];

/** The community list is picked by name: the eight-card fixture, or nothing to show. */
const COMMUNITY_SETS = { eight: COMMUNITIES, none: [] as Community[] };
type CommunitySetKey = keyof typeof COMMUNITY_SETS;

/** Story args: the community list is picked by name, the rest are real props. */
type CommunitiesStoryArgs = {
  communitySet: CommunitySetKey;
  isOwnProfile: boolean;
  loading: boolean;
};

const BASE: CommunitiesStoryArgs = {
  communitySet: "eight",
  isOwnProfile: false,
  loading: false,
};

function renderTab({
  communitySet,
  isOwnProfile,
  loading,
  chrome,
  labelSuffix,
}: CommunitiesStoryArgs & { chrome?: boolean; labelSuffix?: string }) {
  return (
    <StProfileCommunitiesTab
      profile={PROFILE}
      communities={COMMUNITY_SETS[communitySet]}
      isOwnProfile={isOwnProfile}
      loading={loading}
      chrome={chrome}
      labelSuffix={labelSuffix}
    />
  );
}

const meta = {
  title: "Web/Pages/Profile/Communities Tab",
  component: StProfileCommunitiesTab,
  parameters: { layout: "fullscreen" },
  argTypes: {
    communitySet: {
      control: "select",
      options: ["eight", "none"],
      description:
        "Which community fixture is passed \u{2014} the eight-card set (owner/admin/member roles), or an empty list.",
    },
    isOwnProfile: {
      control: "boolean",
      description: "Switches the owner-only card actions and the empty-state copy.",
    },
    loading: { control: "boolean", description: "Renders the skeleton grid." },
  },
  args: BASE,
  render: renderTab,
} satisfies Meta<CommunitiesStoryArgs>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

const CATALOG: { label: string; args: CommunitiesStoryArgs }[] = [
  { label: "member view", args: BASE },
  { label: "own profile", args: { ...BASE, isOwnProfile: true } },
  { label: "empty \u{2014} owner", args: { communitySet: "none", isOwnProfile: true, loading: false } },
  { label: "empty \u{2014} member", args: { communitySet: "none", isOwnProfile: false, loading: false } },
  { label: "loading", args: { communitySet: "none", isOwnProfile: false, loading: true } },
];

/**
 * Every state at once. `Default` flips between them from the Controls panel; this keeps the
 * member view, the owner view, both empty states and the skeleton in the render + a11y +
 * visual-diff gates. `chrome={false}` so stacking does not emit N `<main>` landmarks, and
 * `labelSuffix` makes each copy's `nav[aria-label="Profile sections"]` uniquely named --
 * axe's `landmark-unique` compares accessible names, so a fixed label would fail N times.
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
