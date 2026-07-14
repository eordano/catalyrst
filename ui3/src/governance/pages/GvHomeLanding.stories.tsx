import type { Meta, StoryObj } from "@storybook/react-vite";
import GvHomeLanding, {
  type GvHomeActivity,
  type GvHomeDelegate,
  type GvHomeGrant,
  type GvHomeProposal,
  type GvHomeTopVoter,
} from "./GvHomeLanding";

const ENDING_SOON: GvHomeProposal[] = [
  {
    id: 1,
    title: "Add a new Catalyst node operated by the DAO in the EU region",
    author: "0x7c…a4e1",
    hue: 210,
    type: "Catalyst Node",
    tone: "blue",
    votes: 184,
    comments: 26,
    time: "Ends in 2 hours",
    urgent: true,
    met: false,
    vp: "1,240,000",
  },
  {
    id: 2,
    title: "Grant Request: Decentraland Builders Hackathon Season 5",
    author: "buildersdao.dcl",
    hue: 268,
    type: "Grant Request",
    tone: "purple",
    votes: 311,
    comments: 47,
    time: "Ends in 18 hours",
    urgent: true,
    met: true,
    vp: "3,902,114",
  },
  {
    id: 3,
    title: "Should we lower the Grant proposal vote-power threshold to 1M VP?",
    author: "0x12…9f0c",
    hue: 30,
    type: "Poll",
    tone: "orange",
    votes: 642,
    comments: 88,
    time: "Ends in 1 day",
    met: true,
    vp: "5,118,440",
  },
  {
    id: 4,
    title: "Enact the Q2 Treasury rebalancing toward the Grants reserve",
    author: "governance.dcl",
    hue: 30,
    type: "Governance",
    tone: "orange",
    votes: 529,
    comments: 54,
    time: "Ends in 3 days",
    met: false,
    vp: "820,500",
  },
  {
    id: 5,
    title: "Add Genesis Plaza Fountain as a Point of Interest",
    author: "0xab…77d3",
    hue: 130,
    type: "Point of Interest",
    tone: "green",
    votes: 208,
    comments: 19,
    time: "Ends in 4 days",
    met: true,
    vp: "1,560,300",
  },
];

const GRANTS: GvHomeGrant[] = [
  {
    id: 1,
    title: "Decentraland Game Jam — Tooling & Prizes",
    category: "Core Unit",
    hue: 268,
    size: "$240,000",
    pct: 64,
    months: 9,
    update: "Update #6 · Milestone 4 shipped",
  },
  {
    id: 2,
    title: "Open-source SDK7 component library for builders",
    category: "Platform",
    hue: 210,
    size: "$120,000",
    pct: 38,
    months: 12,
    update: "Update #3 · On track",
  },
  {
    id: 3,
    title: "Weekly community events & town-hall production",
    category: "Social Media",
    hue: 30,
    size: "$96,000",
    pct: 81,
    months: 6,
    update: "Update #9 · Ahead of schedule",
  },
  {
    id: 4,
    title: "Wearable & emote creator onboarding program",
    category: "Documentation",
    hue: 130,
    size: "$60,000",
    pct: 22,
    months: 8,
    update: "Update #2 · Kicked off",
  },
];

const DELEGATES: GvHomeDelegate[] = [
  { address: "0xc1b8…4d53", name: "metahero.dcl", hue: 268, lastVoted: "2d", timesVoted: 412, pickedBy: 1840, totalVP: "8,204,113" },
  { address: "0x3fB3…A13B", name: "morris.dcl", hue: 200, lastVoted: "5d", timesVoted: 388, pickedBy: 1502, totalVP: "6,910,540" },
  { address: "0x76fb…7cb0", name: "facilitator.dcl", hue: 130, lastVoted: "1d", timesVoted: 521, pickedBy: 2210, totalVP: "5,448,902" },
  { address: "0x9f3c…7a21", name: "0x9f3c…7a21", hue: 48, lastVoted: "3d", timesVoted: 264, pickedBy: 980, totalVP: "4,120,775" },
  { address: "0x55a1…1b2a", name: "builderdao.dcl", hue: 305, lastVoted: "6d", timesVoted: 197, pickedBy: 742, totalVP: "3,388,210" },
  { address: "0xab12…77d3", name: "0xab12…77d3", hue: 12, lastVoted: "8d", timesVoted: 143, pickedBy: 560, totalVP: "2,901,004" },
  { address: "0x12c9…9f0c", name: "polly.dcl", hue: 96, lastVoted: "12d", timesVoted: 121, pickedBy: 431, totalVP: "2,015,330" },
  { address: "0x7ce4…a4e1", name: "0x7ce4…a4e1", hue: 340, lastVoted: "4d", timesVoted: 98, pickedBy: 312, totalVP: "1,744,820" },
  { address: "0x3ec9…c901", name: "guardian.dcl", hue: 168, lastVoted: "9d", timesVoted: 77, pickedBy: 240, totalVP: "1,209,650" },
];

const TOP_VOTERS: GvHomeTopVoter[] = [
  { rank: 1, name: "metahero.dcl", hue: 268, votes: 96 },
  { rank: 2, name: "morris.dcl", hue: 200, votes: 88 },
  { rank: 3, name: "0x76fb…7cb0", hue: 130, votes: 81 },
  { rank: 4, name: "polly.dcl", hue: 96, votes: 74 },
  { rank: 5, name: "guardian.dcl", hue: 168, votes: 69 },
  { rank: 6, name: "0x9f3c…7a21", hue: 48, votes: 61 },
];

const ACTIVITY: GvHomeActivity[] = [
  { id: 1, kind: "vote", hue: 268, html: "<b>metahero.dcl</b> voted on <b>Add a new Catalyst node…</b>", date: "2 min ago" },
  { id: 2, kind: "comment", html: "New comment on <b>Grant Request: Builders Hackathon…</b>", date: "11 min ago" },
  { id: 3, kind: "proposal", hue: 200, html: "<b>buildersdao.dcl</b> published a new Proposal <b>Hackathon Season 5</b>", date: "34 min ago" },
  { id: 4, kind: "vote", hue: 130, html: "<b>facilitator.dcl</b> voted on <b>Q2 Treasury rebalancing</b>", date: "1 hr ago" },
  { id: 5, kind: "delegation", hue: 48, html: "<b>0x9f3c…7a21</b> delegated to <b>metahero.dcl</b>", date: "2 hr ago" },
  { id: 6, kind: "finished", html: "Voting on Proposal <b>Genesis Plaza Fountain</b> has ended with the outcome <b>Accepted</b>", date: "3 hr ago" },
  { id: 7, kind: "comment", html: "New comment on <b>Lower the Grant VP threshold to 1M</b>", date: "4 hr ago" },
  { id: 8, kind: "update", hue: 30, html: "<b>governance.dcl</b> published an Update on the Project <b>Game Jam Tooling</b>", date: "5 hr ago" },
  { id: 9, kind: "vesting", html: "A vesting contract for the Project <b>SDK7 component library</b> has been created. $120,000 will be vested in 12 months", date: "6 hr ago" },
  { id: 10, kind: "delegation", hue: 305, html: "<b>polly.dcl</b> removed delegation from <b>morris.dcl</b>", date: "8 hr ago" },
];

const meta = {
  title: "Governance/Pages/Home",
  component: GvHomeLanding,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof GvHomeLanding>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => (
    <GvHomeLanding
      endingSoon={ENDING_SOON}
      grants={GRANTS}
      delegates={DELEGATES}
      topVoters={TOP_VOTERS}
      activity={ACTIVITY}
    />
  ),
};

export const EmptyOpenProposals: Story = {
  render: () => (
    <GvHomeLanding
      endingSoon={[]}
      grants={GRANTS}
      delegates={DELEGATES}
      topVoters={TOP_VOTERS}
      activity={ACTIVITY}
    />
  ),
};
