import type { Meta, StoryObj } from "@storybook/react-vite";
import GvVotingPowerDelegationDetail, {
  type GvDelegationCandidate,
  type GvDelegationVote,
  type GvVpDistribution,
} from "./GvVotingPowerDelegationDetail";

const CANDIDATE: GvDelegationCandidate = {
  address: "0x7c4f…a4e1",
  name: "metahero.dcl",
  hue: 268,
  bio:
    "DAO contributor and long-time community builder. I have been an active " +
    "voter since the Genesis launch and care deeply about keeping governance " +
    "accessible to everyone in Decentraland.",
  involvement:
    "Member of the Grants Support Squad and a frequent forum moderator. I " +
    "review grant updates and help new applicants navigate the proposal flow.",
  motivation:
    "I want delegated VP to be used transparently. Every vote I cast comes " +
    "with a written rationale posted to the forum so delegators can hold me " +
    "accountable.",
  vision:
    "A self-sustaining DAO where treasury spend is predictable, grant outcomes " +
    "are measured, and the platform's roadmap is set by its users.",
  most_important_issue:
    "Closing the loop on grant accountability — funding should track delivered " +
    "milestones, not promises.",
  links: [
    "https://forum.decentraland.org/u/metahero",
    "https://twitter.com/metahero_dcl",
    "https://github.com/metahero",
  ],
  relevant_skills: ["Governance", "Grants Review", "Community", "Solidity", "Treasury"],
};

const VP_DISTRIBUTION: GvVpDistribution = {
  total: 184_200,
  own: 142_000,
  delegated: 42_200,
  mana: 96_400,
  names: 18_000,
  l1Wearables: 7_800,
  land: 38_000,
  estate: 16_000,
  rental: 8_000,
};

const VOTES: GvDelegationVote[] = [
  { id: "v1", title: "Add a new Catalyst node operated by the DAO in the EU region", choice: "Yes", match: true },
  { id: "v2", title: "Grant Request: Decentraland Builders Hackathon Season 5", choice: "Yes", match: true },
  { id: "v3", title: "Should we lower the Grant proposal vote-power threshold to 1M VP?", choice: "No", match: false },
  { id: "v4", title: "Enact the Q2 Treasury rebalancing toward the Grants reserve", choice: "Abstain", match: undefined },
  { id: "v5", title: "Add Genesis Plaza Fountain as a Point of Interest", choice: "Yes", match: true },
  { id: "v6", title: "Ban the name “decentraland-official” from the registry", choice: "Yes", match: true },
];

const meta = {
  title: "Governance/Components/Voting Power delegation",
  component: GvVotingPowerDelegationDetail,
  parameters: { layout: "fullscreen" },
  args: {
    candidate: CANDIDATE,
    vpDistribution: VP_DISTRIBUTION,
    votes: VOTES,
    matchPercentage: 84,
    activeSince: "March, 2021",
    userVP: "12,480",
  },
} satisfies Meta<typeof GvVotingPowerDelegationDetail>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const Loading: Story = {
  args: { loading: true },
};

export const NoVotes: Story = {
  args: {
    votes: [],
    matchPercentage: 0,
  },
};
