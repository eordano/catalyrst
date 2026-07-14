import type { Meta, StoryObj } from "@storybook/react-vite";
import StProfileReferralRewardsTab from "./StProfileReferralRewardsTab";
import type { ReferralData, ReferralProfile } from "./StProfileReferralRewardsTab";

const PROFILE: ReferralProfile = {
  address: "0x2fa1b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0",
  name: "PixelNomad",
  hasClaimedName: true,
  nameColor: "#FF8362",
};

const accepted = (n: number): ReferralData => ({
  invitedUsersAccepted: n,
  invitedUsersAcceptedViewed: n,
  rewardImages: [],
});

/** The referral payload is picked by name; `none` passes `null`, as the non-ready states do. */
const DATA = {
  none: null,
  invited0: accepted(0),
  invited22: accepted(22),
  invited100: accepted(100),
} satisfies Record<string, ReferralData | null>;
type DataKey = keyof typeof DATA;

/** Every value the component's `state` prop accepts. */
const STATES = ["ready", "loading", "error"] as const;
type StateKey = (typeof STATES)[number];

/** Story args: the referral payload is picked by name, `state` is the real prop. */
type ReferralStoryArgs = { dataPreset: DataKey; state: StateKey };

const BASE: ReferralStoryArgs = { dataPreset: "invited22", state: "ready" };

function renderTab({
  dataPreset,
  state,
  chrome,
  labelSuffix,
}: ReferralStoryArgs & { chrome?: boolean; labelSuffix?: string }) {
  return (
    <StProfileReferralRewardsTab
      profile={PROFILE}
      data={DATA[dataPreset]}
      state={state}
      chrome={chrome}
      labelSuffix={labelSuffix}
    />
  );
}

const meta = {
  title: "Web/Pages/Profile/Referral Rewards Tab",
  component: StProfileReferralRewardsTab,
  parameters: { layout: "fullscreen" },
  argTypes: {
    dataPreset: {
      control: "select",
      options: ["none", "invited0", "invited22", "invited100"],
      description:
        "How many accepted invites the payload reports \u{2014} none (null), just started, mid-progress, or every tier unlocked.",
    },
    state: {
      control: "inline-radio",
      options: STATES,
      description: "`ready` renders the tier track; `loading` the skeleton; `error` the anonymous view.",
    },
  },
  args: BASE,
  render: renderTab,
} satisfies Meta<ReferralStoryArgs>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

const CATALOG: { label: string; args: ReferralStoryArgs }[] = [
  { label: "mid-progress", args: BASE },
  { label: "just started", args: { dataPreset: "invited0", state: "ready" } },
  { label: "all unlocked", args: { dataPreset: "invited100", state: "ready" } },
  { label: "loading", args: { dataPreset: "none", state: "loading" } },
  { label: "anonymous", args: { dataPreset: "none", state: "error" } },
];

/**
 * Every state at once. `Default` flips between them from the Controls panel; this keeps all
 * three tier-track positions, the skeleton and the anonymous view in the render + a11y +
 * visual-diff gates. `chrome={false}` so stacking does not emit N `<main>` landmarks, and
 * `labelSuffix` makes each copy's two named landmarks -- `nav[aria-label="Profile sections"]`
 * and the `role="region"` reward-journey scroller -- uniquely named; axe's `landmark-unique`
 * compares accessible names, so fixed labels would fail N times.
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
