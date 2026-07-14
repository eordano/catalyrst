import type { ComponentProps } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import GvProjectDetail, { type GvProject } from "./GvProjectDetail";

const PROJECT: GvProject = {
  id: "1f5c2a9e-0b3d-4c7a-9e21-7f8a1c2b3d4e",
  proposal_id: "0xa7b3\u{2026}proposal",
  title: "Decentraland Builders Hackathon Season 5",
  status: "in_progress",
  ongoingDays: 84,
  about:
    "This grant funds the fifth season of the Decentraland Builders Hackathon: a six-week program supporting independent creators building scenes, smart wearables, and SDK tooling. Funds cover prize pools, mentor stipends, and the live demo-day infrastructure.\n\nThe team will run weekly office hours, ship two starter templates to the asset library, and publish a post-mortem with retention metrics for every cohort.",
  links: [
    { id: "l1", label: "Project Website", url: "https://hackathon.dcl" },
    { id: "l2", label: "GitHub Repository", url: "https://github.com/example/dcl-hackathon" },
    { id: "l3", label: "Demo Day Recording", url: "https://youtube.com/watch?v=example" },
  ],
  personnel: [
    {
      id: "p1",
      name: "Mariana Vex",
      address: "0x9f3c\u{2026}7a21",
      role: "Project Lead",
      about:
        "Five years organising builder programs across web3 gaming. Leads scope, milestones, and the mentor network.",
    },
    {
      id: "p2",
      name: "buildersdao.dcl",
      address: "0x12c4\u{2026}b80d",
      role: "Operations",
      about: "Handles prize disbursement, KYC, and the demo-day logistics.",
    },
    {
      id: "p3",
      name: "Theo Nakamura",
      address: null,
      role: "SDK Mentor",
      about: "Maintains the starter templates and reviews scene submissions.",
    },
  ],
  milestones: [
    { id: "m1", date: "2026-04-15", title: "Cohort kickoff & onboarding", description: "Open applications, select 40 teams, run kickoff workshop." },
    { id: "m2", date: "2026-05-20", title: "Mid-program checkpoint", description: "Ship two starter templates, host first office-hours block." },
    { id: "m3", date: "2026-06-30", title: "Demo Day & report", description: "Live demo-day broadcast and published retention post-mortem." },
  ],
  funding: {
    enactedLabel: "3 months ago",
    endLabel: "in 2 months",
    total: "120,000",
    token: "USD",
    vestedAmount: "78,400",
    vestedPct: 65,
    releasedAmount: "54,200",
    releasedPct: 45,
    nextVested: { time: 12, unit: "days", amount: "20,000" },
  },
  vestings: [
    { id: "v1", label: "Current vesting", url: "https://etherscan.io/address/0xv1" },
    { id: "v2", label: "Past vesting", url: "https://etherscan.io/address/0xv2" },
  ],
};

const FINISHED_PROJECT: GvProject = {
  id: "9c1d\u{2026}",
  proposal_id: "0xc1d2\u{2026}proposal",
  title: "Decentraland SDK7 Migration Toolkit",
  status: "finished",
  ongoingDays: 0,
  about:
    "A toolkit and documentation suite to migrate legacy SDK6 scenes to SDK7. Delivered an automated converter, a compatibility linter, and three migration guides.",
  links: [
    { id: "l1", label: "Documentation", url: "https://docs.dcl" },
    { id: "l2", label: "Source Code", url: "https://github.com/example/sdk7-toolkit" },
  ],
  personnel: [
    { id: "p1", name: "Lena Cruz", address: "0x4a2b\u{2026}99fe", role: "Lead Engineer", about: "Built the automated converter and the compatibility linter." },
  ],
  milestones: [
    { id: "m1", date: "2025-11-10", title: "Converter alpha", description: "First working pass of the SDK6\u{2192}SDK7 converter." },
    { id: "m2", date: "2026-01-20", title: "Final release", description: "Stable converter, linter, and published guides." },
  ],
  funding: {
    enactedLabel: "8 months ago",
    endLabel: "1 month ago",
    total: "60,000",
    token: "USD",
    vestedAmount: "60,000",
    vestedPct: 100,
    releasedAmount: "60,000",
    releasedPct: 100,
    nextVested: { time: 0, unit: "days", amount: "0" },
  },
  vestings: [{ id: "v1", label: "Current vesting", url: "https://etherscan.io/address/0xv1" }],
};

/** The project blobs the stories used to differ by, now pickable by name. */
const PROJECTS = { inProgress: PROJECT, finished: FINISHED_PROJECT };
type ProjectKey = keyof typeof PROJECTS;
const PROJECT_KEYS: ProjectKey[] = ["inProgress", "finished"];

type ProjectDetailProps = ComponentProps<typeof GvProjectDetail>;

/**
 * The story args: the project blob is picked by name, everything else is a real prop.
 * The synthetic key must not shadow a real prop name -- `component:` is type-checked against
 * `ComponentType<ProjectStoryArgs>`, so these args have to stay assignable to the real props.
 */
type ProjectStoryArgs = Omit<ProjectDetailProps, "project"> & { projectFixture: ProjectKey };

const meta = {
  title: "Governance/Pages/Project Detail",
  component: GvProjectDetail,
  parameters: { layout: "fullscreen" },
  argTypes: {
    projectFixture: {
      control: "select",
      options: PROJECT_KEYS,
      description: "Which project fixture drives the page \u{2014} an ongoing grant or a delivered one.",
    },
    loading: { control: "boolean" },
    notFound: { control: "boolean", description: "Also what a missing project renders." },
  },
  args: { projectFixture: "inProgress", loading: false, notFound: false },
  // The page latches its initial update index into useState from the project blob, so a key
  // derived from the fixture is needed for the control to actually swap projects on re-render.
  render: ({ projectFixture, ...rest }) => (
    <GvProjectDetail key={projectFixture} project={PROJECTS[projectFixture]} {...rest} />
  ),
} satisfies Meta<ProjectStoryArgs>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

/*
 * Reachable from the `projectFixture` control like the in-progress blob, but kept as its own
 * export because the story id is a hardcoded external consumer: tools/screen-tour/add-story-links.mts
 * lists `governance-pages-project-detail--finished` in its MAPS and only console.warns when an id
 * stops resolving, so dropping the export would silently break that deep link.
 */
export const Finished: Story = { args: { projectFixture: "finished" } };

/** The loading skeleton, before the project blob resolves. */
export const Loading: Story = { args: { loading: true } };

/** Also what an unknown project id renders. */
export const NotFoundState: Story = { args: { notFound: true } };
