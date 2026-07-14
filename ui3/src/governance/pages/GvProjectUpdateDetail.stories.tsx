import type { ComponentProps } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import GvProjectUpdateDetail, {
  type GvProjectUpdate,
  type GvUpdateComment,
  type GvUpdateProject,
} from "./GvProjectUpdateDetail";

const PROJECT: GvUpdateProject = {
  id: "0x9f3c-7a21",
  title: "Decentraland Builders Hackathon Season 5",
  authorHue: 268,
};

const UPDATE: GvProjectUpdate = {
  id: "u-0007",
  index: 4,
  status: "late",
  health: "onTrack",
  author: "buildersdao.dcl",
  completion_date: "Jun 12, 2026",
  updated_at: "Jun 13, 2026",
  due_date: "Jun 09, 2026",
  due_amount: "3 days",
  introduction:
    "Season 5 wrapped its mid-program checkpoint this week. We onboarded 38 new builders, ran two live office-hours sessions, and shipped the first batch of mentor-paired scenes to a staging world for review.",
  highlights: [
    "38 first-time builders completed SDK onboarding (target was 30).",
    "12 playable scenes deployed to the staging world for mentor review.",
    "Public retention dashboard is live and tracking 30-day activity.",
  ],
  blockers:
    "Mentor availability dipped during the second week, which pushed three review sessions back. We have since added two backup mentors from the previous cohort to keep reviews on schedule.",
  next_steps:
    "Finalize the community-choice award shortlist, publish the 60-day retention snapshot, and prepare the post-program write-up with per-scene metrics.",
  additional_notes:
    "Retention dashboard and the staging world coordinates are linked in the forum thread for anyone who wants to playtest the submitted scenes.",
  financial_records: [
    {
      category: "Operational",
      records: [
        { description: "Mentor stipends (cohort)", receiver: "0x55\u{2026}1b2a", token: "USDC", amount: 6000, link: "#tx1" },
        { description: "Office-hours hosting", receiver: "0x71\u{2026}9c0d", token: "USDC", amount: 1200, link: "" },
      ],
    },
    {
      category: "Prizes",
      records: [
        { description: "Tier 3 prize pool top-up", receiver: "0x9a\u{2026}4e51", token: "MANA", amount: 8500, link: "#tx3" },
      ],
    },
  ],
  funds: {
    released: "$24,000.00",
    releasedTxCount: 3,
    releasedTime: "Jun 11, 2026",
    disclosed: "$15,700.00",
    undisclosed: "$8,300.00",
  },
  discourse_topic_id: 81422,
};

/** The update blobs the stories used to differ by, now pickable by name. */
const UPDATES = {
  onTrack: UPDATE,
  atRisk: { ...UPDATE, health: "atRisk" },
};
type UpdateKey = keyof typeof UPDATES;
const UPDATE_KEYS: UpdateKey[] = ["onTrack", "atRisk"];

const COMMENTS: GvUpdateComment[] = [
  {
    id: 1,
    name: "metaverse-mike.dcl",
    hue: 200,
    validated: true,
    time: "2 days ago",
    text: "Great checkpoint. The staging-world playtest was smooth and the mentor pairing is clearly paying off in scene quality.",
  },
  {
    id: 2,
    name: "0xab\u{2026}77d3",
    hue: 12,
    validated: false,
    time: "1 day ago",
    text: "Appreciate the public retention dashboard \u{2014} that transparency is exactly what we asked for last season. Looking forward to the 60-day snapshot.",
  },
];

type UpdateDetailProps = ComponentProps<typeof GvProjectUpdateDetail>;

/**
 * The story args: the update blob is picked by name, everything else is a real prop.
 * The synthetic key must not shadow a real prop name -- `component:` is type-checked against
 * `ComponentType<UpdateStoryArgs>`, so these args have to stay assignable to the real props.
 */
type UpdateStoryArgs = Omit<UpdateDetailProps, "update"> & { updateFixture: UpdateKey };

const meta = {
  title: "Governance/Pages/Project Update Detail",
  component: GvProjectUpdateDetail,
  parameters: { layout: "fullscreen" },
  argTypes: {
    updateFixture: {
      control: "select",
      options: UPDATE_KEYS,
      description: "Which update fixture drives the page \u{2014} they differ by reported project health.",
    },
    state: {
      control: "inline-radio",
      options: ["ready", "loading", "error"],
      description: "`error` is also what a missing update or project renders.",
    },
  },
  args: { updateFixture: "onTrack", project: PROJECT, comments: COMMENTS, state: "ready" },
  render: ({ updateFixture, ...rest }) => (
    <GvProjectUpdateDetail update={UPDATES[updateFixture]} {...rest} />
  ),
} satisfies Meta<UpdateStoryArgs>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

const CATALOG_STATES: { label: string; props: UpdateDetailProps }[] = [
  { label: "Health: on track", props: { update: UPDATES.onTrack, project: PROJECT, comments: COMMENTS } },
  { label: "Health: at risk", props: { update: UPDATES.atRisk, project: PROJECT, comments: COMMENTS } },
  { label: "Loading", props: { state: "loading" } },
  { label: "Not found", props: { state: "error" } },
];

/**
 * Every state rendered at once. `Default` flips between them with the `updateFixture` and `state`
 * controls; this story keeps all four in the render + a11y + visual-diff gates, since each is a
 * structurally different page (on-track health banner, at-risk health banner, loading spinner,
 * not-found panel).
 */
export const Catalog: Story = {
  name: "Catalog (every state)",
  parameters: {
    controls: { disable: true },
  },
  render: () => (
    <div className="gv" style={{ display: "flex", flexDirection: "column", gap: 48 }}>
      {CATALOG_STATES.map(({ label, props }) => (
        // <section> demotes each page's unnamed header/footer/aside to `generic`
        // (HTML-AAM scoped mapping) so the stack does not invent extra landmarks.
        <section key={label}>
          <div style={{ padding: "8px 16px", opacity: 0.7 }}>{label}</div>
          <GvProjectUpdateDetail {...props} chrome={false} />
        </section>
      ))}
    </div>
  ),
};
