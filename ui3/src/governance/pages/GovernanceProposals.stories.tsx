import type { Meta, StoryObj } from "@storybook/react-vite";
import GovernanceProposals, { type GpProposal } from "./GovernanceProposals";

const PROPOSALS: GpProposal[] = [
  {
    id: 1,
    title: "Add a new Catalyst node operated by the DAO in the EU region",
    category: "catalyst",
    status: "active",
    author: "0x7c\u{2026}a4e1",
    hue: 210,
    forPct: 72,
    againstPct: 28,
    passing: true,
    votes: 184,
    comments: 26,
    time: "Ends in 2 days",
    urgent: true,
  },
  {
    id: 2,
    title: "Grant Request: Decentraland Builders Hackathon Season 5",
    category: "grant",
    status: "active",
    author: "buildersdao.dcl",
    hue: 268,
    forPct: 58,
    againstPct: 42,
    passing: true,
    votes: 311,
    comments: 47,
    time: "Ends in 18 hours",
    urgent: true,
  },
  {
    id: 3,
    title: "Should we lower the Grant proposal vote-power threshold to 1M VP?",
    category: "poll",
    status: "passed",
    author: "0x12\u{2026}9f0c",
    hue: 30,
    forPct: 81,
    againstPct: 19,
    passing: true,
    votes: 642,
    comments: 88,
    time: "Ended 3 days ago",
  },
  {
    id: 4,
    title: "Enact the Q2 Treasury rebalancing toward the Grants reserve",
    category: "governance",
    status: "enacted",
    author: "governance.dcl",
    hue: 30,
    forPct: 67,
    againstPct: 33,
    passing: true,
    votes: 529,
    comments: 54,
    time: "Enacted 1 week ago",
  },
  {
    id: 5,
    title: "Add Genesis Plaza Fountain as a Point of Interest",
    category: "poi",
    status: "rejected",
    author: "0xab\u{2026}77d3",
    hue: 130,
    forPct: 34,
    againstPct: 66,
    passing: false,
    votes: 208,
    comments: 19,
    time: "Ended 5 days ago",
  },
  {
    id: 6,
    title: "Tender: Build the next-gen World content moderation pipeline",
    category: "tender",
    status: "active",
    author: "0x55\u{2026}1b2a",
    hue: 0,
    forPct: 49,
    againstPct: 51,
    passing: false,
    votes: 97,
    comments: 12,
    time: "Ends in 6 days",
  },
  {
    id: 7,
    title: "Linked Wearables Registry: approve CryptoArt Studios collection",
    category: "linked_wearables",
    status: "out_of_budget",
    author: "cryptoart.dcl",
    hue: 48,
    forPct: 71,
    againstPct: 29,
    passing: false,
    votes: 143,
    comments: 8,
    time: "Finished 2 days ago",
  },
  {
    id: 8,
    title: "Ban the name \u{201C}decentraland-official\u{201D} from the registry",
    category: "ban_name",
    status: "passed",
    author: "0x3e\u{2026}c901",
    hue: 305,
    forPct: 93,
    againstPct: 7,
    passing: true,
    votes: 388,
    comments: 31,
    time: "Ended 6 days ago",
  },
];

const CATEGORY_COUNTS: Record<string, number> = {
  all: 1284,
  poi: 63,
  catalyst: 18,
  ban_name: 22,
  linked_wearables: 37,
  hiring: 6,
  council_decision_veto: 4,
  governance: 488,
  grant: 412,
  bidding: 70,
};

const meta = {
  title: "Governance/Pages/Proposals",
  component: GovernanceProposals,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof GovernanceProposals>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => (
    <GovernanceProposals
      proposals={PROPOSALS}
      totalCount={1284}
      categoryCounts={CATEGORY_COUNTS}
    />
  ),
};

export const Empty: Story = {
  render: () => <GovernanceProposals proposals={[]} />,
};
