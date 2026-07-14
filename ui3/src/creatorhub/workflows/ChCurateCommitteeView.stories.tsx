import type { ComponentProps } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";

import ChCurateCommitteeView from "./ChCurateCommitteeView";

type Props = ComponentProps<typeof ChCurateCommitteeView>;
type Row = NonNullable<Props["collections"]>[number];

const rows: Row[] = [
  {
    id: "c1",
    name: "Genesis Threads",
    type: "standard",
    status: "under_review",
    count: 12,
    owner: "0xa1b2\u{2026}44ff",
    curationStatus: "to_review",
    assignee: null,
    date: "Review request",
    ago: "2 hours ago",
    forumLink: null,
    forumTopicId: 4821,
    thumbs: [
      "linear-gradient(135deg,#438fff,#2f004d)",
      "linear-gradient(135deg,#ff2d55,#350447)",
      "linear-gradient(135deg,#ff4bed,#220040)",
      "linear-gradient(135deg,#982de2,#1a0a2e)",
    ],
    comments: [],
  },
  {
    id: "c2",
    name: "Neon Streetwear Drop",
    type: "standard",
    status: "under_review",
    count: 8,
    owner: "0xc3d4\u{2026}21aa",
    curationStatus: "under_review",
    assignee: "0x9f3c4d1e7a2188cf90b3a6e7c4d5f6a7b8c9d0e1",
    assigneeName: "kira.eth",
    you: true,
    date: "Review request",
    ago: "5 hours ago",
    forumLink: null,
    forumTopicId: 4822,
    thumbs: [
      "linear-gradient(135deg,#1f8a70,#06231c)",
      "linear-gradient(135deg,#ff743a,#3a1500)",
      "linear-gradient(135deg,#73d3d3,#062a2a)",
      "linear-gradient(135deg,#ffc647,#3a2c00)",
    ],
    comments: [
      {
        id: "cm1",
        collection_id: "c2",
        author: "0x7c2d1e0f9a8b7c6d5e4f3a2b1c0d9e8f70615243",
        authorName: "marco.eth",
        decision: "approved",
        raw: "Meshes look clean and the hoodie rig weights are solid. Good to go from my side.",
        topic_id: 4822,
        created_at: "2026-06-30T10:00:00Z",
      },
    ],
  },
  {
    id: "c3",
    name: "Aether Linked Wearables",
    type: "third_party",
    status: null,
    isProgrammatic: true,
    count: 24,
    owner: null,
    curationStatus: "to_review",
    assignee: null,
    date: "Published",
    ago: "1 day ago",
    forumLink: null,
    forumTopicId: null,
    thumbs: [
      "linear-gradient(135deg,#b05cff,#2f004d)",
      "linear-gradient(135deg,#438fff,#06231c)",
      "linear-gradient(135deg,#ff2d55,#3a1500)",
      "linear-gradient(135deg,#34ce76,#062a2a)",
    ],
    comments: [],
  },
];

const activeRow = rows[1]!;
const noop = () => {};

const APPROVE_COMMENT = "Great silhouette work and clean topology \u{2014} approving.";
const REJECT_COMMENT =
  "The feet mesh clips through the ankle bone in the walk cycle \u{2014} please fix and resubmit.";

const meta = {
  title: "CreatorHub/Workflows/Curate Committee",
  component: ChCurateCommitteeView,
  parameters: { layout: "fullscreen" },
  argTypes: {
    view: {
      control: "select",
      options: ["dashboard", "assigning", "reviewing", "commenting", "deciding", "decided"],
      description: "Which screen of the curation flow renders.",
    },
    step: {
      control: "select",
      options: ["dashboard", "review", "comment", "deciding", "decided"],
    },
    decision: { control: "inline-radio", options: ["approved", "rejected"] },
    isCommittee: { control: "boolean" },
    activeId: { control: "text" },
    builderHref: { control: "text" },
    draftComment: { control: "text" },
    authorName: { control: "text" },
    error: { control: "text" },
    collections: { control: "object" },
    postedComment: {
      control: "object",
      description: "Set on the decided screen to quote the comment that was posted.",
    },
  },
  args: {
    view: "dashboard",
    step: "dashboard",
    isCommittee: true,
    collections: rows,
    activeRow,
    activeId: activeRow.id,
    builderHref: "#",
    decision: "approved",
    draftComment: APPROVE_COMMENT,
    authorName: "kira.eth",
    onDashboardClick: noop,
    onBack: noop,
    onDraftApprove: noop,
    onDraftReject: noop,
    onChangeComment: noop,
    onSubmit: noop,
    onBackToQueue: noop,
  },
} satisfies Meta<typeof ChCurateCommitteeView>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

type CurateProps = ComponentProps<typeof ChCurateCommitteeView>;

/**
 * Both rejection variants live in the catalog now that the panels derive their ids from `useId()`.
 * Two instances of the same screen no longer collide on one `aria-labelledby` target, and each
 * screen's title already varies with the decision ("Add a comment" vs "Add a reason", "Approved ..."
 * vs "Rejected ..."), so the two regions keep distinct accessible names and landmark-unique passes.
 */
const CASES: { label: string; args: Partial<CurateProps> }[] = [
  { label: "Dashboard queue", args: { view: "dashboard", step: "dashboard" } },
  { label: "Reviewing a collection", args: { view: "reviewing", step: "review" } },
  {
    label: "Commenting \u{B7} approving",
    args: { view: "commenting", step: "comment", decision: "approved" },
  },
  {
    label: "Commenting \u{B7} rejecting",
    args: {
      view: "commenting",
      step: "comment",
      decision: "rejected",
      draftComment: REJECT_COMMENT,
    },
  },
  { label: "Deciding (in flight)", args: { view: "deciding", step: "deciding" } },
  {
    label: "Decided \u{B7} approved",
    args: {
      view: "decided",
      step: "decided",
      decision: "approved",
      postedComment: { postId: 55123, link: "#", raw: APPROVE_COMMENT },
    },
  },
  {
    label: "Decided \u{B7} rejected",
    args: { view: "decided", step: "decided", decision: "rejected" },
  },
];

export const Catalog: Story = {
  name: "Catalog (every screen)",
  parameters: { controls: { disable: true } },
  render: (args) => (
    <div style={{ display: "flex", flexDirection: "column", gap: 40, padding: 24 }}>
      {CASES.map((c) => (
        <section key={c.label} aria-label={c.label}>
          <div style={{ font: "600 13px var(--font-sans)", opacity: 0.7, margin: "0 0 8px" }}>
            {c.label}
          </div>
          <ChCurateCommitteeView {...args} {...c.args} />
        </section>
      ))}
    </div>
  ),
};
