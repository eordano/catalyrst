import type { Meta, StoryObj } from "@storybook/react-vite";
import StProfilePlacesTab from "./StProfilePlacesTab";
import type { Place, Profile } from "./StProfilePlacesTab";

const PROFILE: Profile = {
  address: "0x2fa1b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0",
  name: "PixelNomad",
  hasClaimedName: true,
  nameColor: "#FF8362",
  mutualCount: 3,
};

const PLACES: Place[] = [
  {
    id: "p1",
    title: "Nomad's Plaza",
    description:
      "An open-air gathering spot with rotating DJ sets every evening. Grab a seat by the fountain, browse the gallery wall, or jump into one of the nightly scene jams. Built and curated by PixelNomad.",
    image: "linear-gradient(150deg, hsl(280 70% 52%) 0%, hsl(320 60% 28%) 100%)",
    base_position: "-42,18",
    likes: 1280,
    user_count: 34,
    owner: "0x2fa1b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0",
    contact_name: "PixelNomad",
  },
  {
    id: "p2",
    title: "Aurora Gardens",
    description:
      "A calm botanical world for contemplative wandering. No quests, no noise \u{2014} just light, sound and shifting colour.",
    image: "linear-gradient(150deg, hsl(170 70% 50%) 0%, hsl(210 60% 28%) 100%)",
    world: true,
    world_name: "aurora.dcl.eth",
    likes: 642,
    user_count: 9,
    owner: "0x2fa1b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0",
    contact_name: "PixelNomad",
  },
  {
    id: "p3",
    title: "The Glitch Arcade",
    description: "Retro cabinets, leaderboard wars and a hidden speakeasy upstairs.",
    image: "linear-gradient(150deg, hsl(20 80% 54%) 0%, hsl(350 60% 30%) 100%)",
    base_position: "73,-12",
    likes: 305,
    user_count: 2,
    owner: "0x2fa1b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0",
    contact_name: "PixelNomad",
  },
  {
    id: "p4",
    title: "Founders Hall",
    description: "",
    image: "linear-gradient(150deg, hsl(45 75% 55%) 0%, hsl(30 60% 30%) 100%)",
    base_position: "12,12",
    likes: 88,
    user_count: 0,
    owner: "0x2fa1b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0",
    contact_name: "PixelNomad",
  },
];

/** The place list is picked by name: the four-card fixture, or nothing to show. */
const PLACE_SETS = { four: PLACES, none: [] as Place[] };
type PlaceSetKey = keyof typeof PLACE_SETS;

/** Which empty copy the tab shows. */
const EMPTY_VIEWS = ["owner", "favorites", "member"] as const;

/**
 * Story args: the place list is picked by name through the synthetic `placeSet` key, so the
 * type stays assignable to the component's own props (which `component:` is checked against).
 */
type PlacesStoryArgs = {
  placeSet: PlaceSetKey;
  isOwnProfile: boolean;
  loading: boolean;
  emptyView?: (typeof EMPTY_VIEWS)[number];
};

const BASE: PlacesStoryArgs = { placeSet: "four", isOwnProfile: false, loading: false };

function renderTab({
  placeSet,
  isOwnProfile,
  loading,
  emptyView,
  chrome,
  labelSuffix,
}: PlacesStoryArgs & { chrome?: boolean; labelSuffix?: string }) {
  return (
    <StProfilePlacesTab
      profile={PROFILE}
      places={PLACE_SETS[placeSet]}
      isOwnProfile={isOwnProfile}
      loading={loading}
      emptyView={emptyView ?? null}
      chrome={chrome}
      labelSuffix={labelSuffix}
    />
  );
}

const meta = {
  title: "Web/Pages/Profile/Places Tab",
  component: StProfilePlacesTab,
  parameters: { layout: "fullscreen" },
  argTypes: {
    placeSet: {
      control: "select",
      options: ["four", "none"],
      description: "Which place fixture is passed \u{2014} the four-card set, or an empty list.",
    },
    isOwnProfile: {
      control: "boolean",
      description: "Switches the member tab set for the owner tab set and the owner-only CTAs.",
    },
    loading: { control: "boolean", description: "Renders the skeleton grid." },
    emptyView: {
      control: "select",
      options: EMPTY_VIEWS,
      description: "Which empty-state copy is shown; the empty option leaves the prop unset.",
    },
  },
  args: BASE,
  render: renderTab,
} satisfies Meta<PlacesStoryArgs>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

const CATALOG: { label: string; args: PlacesStoryArgs }[] = [
  { label: "member view", args: BASE },
  { label: "own profile", args: { ...BASE, isOwnProfile: true } },
  {
    label: "empty \u{2014} owner places",
    args: { placeSet: "none", isOwnProfile: true, loading: false, emptyView: "owner" },
  },
  {
    label: "empty \u{2014} favorites",
    args: { placeSet: "none", isOwnProfile: true, loading: false, emptyView: "favorites" },
  },
  {
    label: "empty \u{2014} member",
    args: { placeSet: "none", isOwnProfile: false, loading: false, emptyView: "member" },
  },
  { label: "loading", args: { placeSet: "none", isOwnProfile: false, loading: true } },
];

/**
 * Every state at once. `Default` flips between them from the Controls panel; this keeps the
 * member grid, the owner grid, all three empty-state copies and the skeleton in the render +
 * a11y + visual-diff gates. `chrome={false}` so stacking does not emit N `<main>` landmarks,
 * and `labelSuffix` makes each copy's `nav[aria-label="Profile sections"]` uniquely named --
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
