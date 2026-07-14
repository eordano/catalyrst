import type { ComponentProps } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import GvProposalDetail, {
  type GvProposal,
  type GvProposalComment,
  type GvSurveyRow,
  type GvVoteChoice,
  type GvVoteRationale,
  type GvVpSeries,
} from "./GvProposalDetail";

const PROPOSAL: GvProposal = {
  id: "0x9f3c-7a21",
  type: "grant",
  toneClass: "purple",
  catLabel: "Grant Request",
  catTone: "purple",
  status: "active",
  statusLabel: "Active",
  statusTone: "neutral",
  title: "Grant Request: Decentraland Builders Hackathon Season 5",
  author: "buildersdao.dcl",
  authorHue: 268,
  published: "May 28, 2026 14:10",
  start: "Jun 02, 2026 00:00",
  finish: "Jun 16, 2026 00:00",
  snapshot: "#4b91c2f",
  threshold: "2,000,000",
  thresholdReached: false,
  yourVp: "12,480",
  budget: { size: "$45,000", beneficiary: "0x55\u{2026}1b2a", tier: "Tier 3" },
  description:
    "We're requesting a grant to run the fifth season of the Decentraland Builders Hackathon \u{2014} a six-week program that onboards new creators to the SDK and ships playable scenes to Genesis City.\n\nPrior seasons produced 40+ published scenes and brought ~120 first-time builders into the ecosystem. This season focuses on retention: every participant is paired with a mentor and committed to a public post-mortem.",
};

const FINISHED_POLL: GvProposal = {
  id: "0x12-9f0c",
  type: "poll",
  toneClass: "orange",
  catLabel: "Poll",
  catTone: "orange",
  status: "passed",
  statusLabel: "Passed",
  statusTone: "green",
  title: "Should we lower the Grant proposal vote-power threshold to 1M VP?",
  author: "0x12\u{2026}9f0c",
  authorHue: 30,
  published: "Apr 12, 2026 09:30",
  start: "Apr 14, 2026 00:00",
  finish: "Apr 28, 2026 00:00",
  snapshot: "#7af0d31",
  threshold: "1,000,000",
  thresholdReached: true,
  yourVp: "12,480",
  budget: { size: "\u{2014}", beneficiary: "\u{2014}", tier: "\u{2014}" },
};

/** The proposal blobs the stories used to differ by, now pickable by name. */
const PROPOSALS = { activeGrant: PROPOSAL, finishedPoll: FINISHED_POLL };
type ProposalKey = keyof typeof PROPOSALS;
const PROPOSAL_KEYS: ProposalKey[] = ["activeGrant", "finishedPoll"];

const CHOICES: GvVoteChoice[] = [
  { id: "yes", label: "Yes", pct: 64, vp: "1,420,300", tone: "yes", voted: true },
  { id: "no", label: "No", pct: 36, vp: "798,140", tone: "no", voted: false },
];

const SURVEY: GvSurveyRow[] = [
  { id: "love", label: "Love it", emoji: "\u{1F60D}", count: 142, pct: 58, dir: "up" },
  { id: "neutral", label: "Neutral", emoji: "\u{1F610}", count: 47, pct: 19, dir: "neutral" },
  { id: "concerned", label: "Concerned", emoji: "\u{1F61F}", count: 56, pct: 23, dir: "down" },
];

const RATIONALE: GvVoteRationale[] = [
  {
    id: 1,
    name: "metaverse-mike.dcl",
    hue: 200,
    choice: "Yes",
    tone: "yes",
    vp: "210,400 VP",
    text: "Builders programs consistently bring new creators into Decentraland. The proposed milestones and beneficiary track record justify the budget.",
  },
  {
    id: 2,
    name: "0xab\u{2026}77d3",
    hue: 12,
    choice: "No",
    tone: "no",
    vp: "88,900 VP",
    text: "I support hackathons in principle but $45k feels high for a single season without committed retention metrics. Would vote Yes on a scoped-down Tier 2.",
  },
];

const COMMENTS: GvProposalComment[] = [
  {
    id: 1,
    name: "buildersdao.dcl",
    hue: 268,
    time: "2 days ago",
    text: "Thanks for the early feedback. We've added a retention-tracking milestone to the grant scope and will publish a public dashboard.",
  },
  {
    id: 2,
    name: "metaverse-mike.dcl",
    hue: 200,
    time: "1 day ago",
    text: "Great addition. Looking forward to seeing the dashboard \u{2014} happy to help promote the hackathon across the creator channels.",
  },
];

const VP_SERIES: GvVpSeries = {
  yes: [0, 200000, 480000, 760000, 1010000, 1230000, 1360000, 1420300],
  no: [0, 110000, 260000, 410000, 540000, 650000, 740000, 798140],
  ticks: ["Jun 2", "Jun 9", "Jun 16"],
};

type ProposalDetailProps = ComponentProps<typeof GvProposalDetail>;

/** Everything a rendered proposal needs besides the proposal blob itself. */
const READY_PROPS = {
  choices: CHOICES,
  survey: SURVEY,
  rationales: RATIONALE,
  comments: COMMENTS,
  vpSeries: VP_SERIES,
  totalVotesLabel: "2,218,440 total votes",
  forumUrl:
    "https://forum.decentraland.org/t/grant-request-decentraland-builders-hackathon-season-5",
} satisfies Partial<ProposalDetailProps>;

/**
 * The story args: the proposal blob is picked by name, everything else is a real prop.
 * The synthetic key must not shadow a real prop name -- `component:` is type-checked against
 * `ComponentType<ProposalStoryArgs>`, so these args have to stay assignable to the real props.
 */
type ProposalStoryArgs = Omit<ProposalDetailProps, "proposal"> & { proposalFixture: ProposalKey };

const meta = {
  title: "Governance/Pages/Proposal Detail",
  component: GvProposalDetail,
  parameters: { layout: "fullscreen" },
  argTypes: {
    proposalFixture: {
      control: "select",
      options: PROPOSAL_KEYS,
      description: "Which proposal fixture drives the page.",
    },
    state: {
      control: "inline-radio",
      options: ["ready", "loading", "error"],
      description: "`error` is also what an unknown proposal id renders.",
    },
    totalVotesLabel: { control: "text" },
    forumUrl: { control: "text" },
    commentsTotal: { control: "number" },
  },
  args: { proposalFixture: "activeGrant", state: "ready", ...READY_PROPS },
  render: ({ proposalFixture, ...rest }) => (
    <GvProposalDetail proposal={PROPOSALS[proposalFixture]} {...rest} />
  ),
} satisfies Meta<ProposalStoryArgs>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

/*
 * Reachable from the `proposalFixture` control like the active grant, but kept as its own export
 * because the story id is a hardcoded external consumer: tools/screen-tour/add-story-links.mts
 * lists `governance-pages-proposal-detail--finished` in its MAPS and only console.warns when an id
 * stops resolving, so dropping the export would silently break that deep link.
 */
export const Finished: Story = { args: { proposalFixture: "finishedPoll" } };

/** The loading spinner, before the proposal resolves. */
export const Loading: Story = { args: { state: "loading" } };

/** Also what an unknown proposal id renders. */
export const NotFound: Story = { args: { state: "error" } };
