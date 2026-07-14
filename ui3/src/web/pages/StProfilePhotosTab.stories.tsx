import type { Meta, StoryObj } from "@storybook/react-vite";
import StProfilePhotosTab, { type Photo, type Profile } from "./StProfilePhotosTab";

const PROFILE: Profile = {
  address: "0x742d35cc6634c0532925a3b844bc454e4438f44e",
  name: "metaverse.dcl.eth",
  hasClaimedName: true,
  nameColor: "#FF8362",
  mutualCount: 0,
};

const sceneGrad = (i: number) => {
  const h = (i * 47 + 196) % 360;
  return `linear-gradient(150deg, hsl(${h} 62% 48%) 0%, hsl(${(h + 38) % 360} 55% 26%) 100%)`;
};

function makePhoto(i: number): Photo {
  const scenes = [
    { name: "Genesis Plaza", x: "0", y: "0" },
    { name: "Vegas City", x: "-120", y: "-12" },
    { name: "Dragon City", x: "73", y: "-21" },
    { name: "Wondermine", x: "-9", y: "132" },
    { name: "Fashion Week Plaza", x: "44", y: "-7" },
    { name: "Casino Royale", x: "137", y: "20" },
  ];
  const s = scenes[i % scenes.length] ?? { name: "Genesis Plaza", x: "0", y: "0" };
  return {
    id: `reel-${i}`,
    grad: sceneGrad(i),
    metadata: {
      userName: "metaverse.dcl.eth",
      userAddress: "0x742d35cc6634c0532925a3b844bc454e4438f44e",
      dateTime: "2026-05-30T18:42:00.000Z",
      realm: "main",
      scene: { name: s.name, location: { x: s.x, y: s.y } },
      visiblePeople: [
        {
          userName: "metaverse.dcl.eth",
          userAddress: "0x742d35cc6634c0532925a3b844bc454e4438f44e",
          isGuest: false,
          wearables: [],
        },
        {
          userName: "stardust",
          userAddress: "0x8f3a1b2c4d5e6f7081929394a5b6c7d8e9f0a1b2",
          isGuest: false,
          wearables: [],
        },
      ],
    },
  };
}

const PHOTOS: Photo[] = Array.from({ length: 12 }, (_, i) => makePhoto(i));

/** The photo list is picked by name: the twelve-shot reel, or nothing to show. */
const PHOTO_SETS = { twelve: PHOTOS, none: [] as Photo[] };
type PhotoSetKey = keyof typeof PHOTO_SETS;

/** Story args: the photo list is picked by name, `isOwnProfile` is the real prop. */
type PhotosStoryArgs = { photoSet: PhotoSetKey; isOwnProfile: boolean };

const BASE: PhotosStoryArgs = { photoSet: "twelve", isOwnProfile: false };

function renderTab({
  photoSet,
  isOwnProfile,
  chrome,
  labelSuffix,
}: PhotosStoryArgs & { chrome?: boolean; labelSuffix?: string }) {
  return (
    <StProfilePhotosTab
      profile={PROFILE}
      photos={PHOTO_SETS[photoSet]}
      isOwnProfile={isOwnProfile}
      chrome={chrome}
      labelSuffix={labelSuffix}
    />
  );
}

const meta = {
  title: "Web/Pages/Profile/Photos Tab",
  component: StProfilePhotosTab,
  parameters: { layout: "fullscreen" },
  argTypes: {
    photoSet: {
      control: "select",
      options: ["twelve", "none"],
      description: "Which photo fixture is passed \u{2014} the twelve-shot reel, or an empty list.",
    },
    isOwnProfile: {
      control: "boolean",
      description: "Switches the member tab set for the owner tab set and the empty-state copy.",
    },
  },
  args: BASE,
  render: renderTab,
} satisfies Meta<PhotosStoryArgs>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

const CATALOG: { label: string; args: PhotosStoryArgs }[] = [
  { label: "member view", args: BASE },
  { label: "own profile", args: { ...BASE, isOwnProfile: true } },
  { label: "empty \u{2014} owner", args: { photoSet: "none", isOwnProfile: true } },
  { label: "empty \u{2014} member", args: { photoSet: "none", isOwnProfile: false } },
];

/**
 * Every state at once. `Default` flips between them from the Controls panel; this keeps the
 * member reel, the owner reel and both empty-state copies in the render + a11y + visual-diff
 * gates. `chrome={false}` so stacking does not emit N `<main>` landmarks, and `labelSuffix`
 * makes each copy's `nav[aria-label="Profile sections"]` uniquely named -- axe's
 * `landmark-unique` compares accessible names, so a fixed label would fail N times.
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
