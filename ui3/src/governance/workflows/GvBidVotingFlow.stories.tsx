import type { ComponentProps } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import GvBidVotingFlow, { type GvBid } from "./GvBidVotingFlow";

const BIDS: GvBid[] = [
  {
    id: "b1",
    title: "Bid #1 \u{2014} Decentraland Foundation District Revamp",
    budget: 120000,
    power: 2840219,
    choice: "Yes",
    current: false,
  },
  {
    id: "b2",
    title: "Bid #2 \u{2014} Genesis Plaza Live Events Infrastructure",
    budget: 95000,
    power: 4120880,
    choice: "Yes",
    current: true,
  },
  {
    id: "b3",
    title: "Bid #3 \u{2014} Community-Run Plaza Maintenance & Tooling",
    budget: 84500,
    power: 1903447,
    choice: "Yes",
    current: false,
  },
];

const STATES = ["default", "casting", "error", "redirect"] as const;

const CASES: ComponentProps<typeof GvBidVotingFlow>[] = [
  { state: "default" },
  { state: "casting" },
  { state: "error", retryTimer: "30s" },
  { state: "redirect" },
];

const meta = {
  title: "Governance/Workflows/Bid voting",
  component: GvBidVotingFlow,
  parameters: { layout: "fullscreen" },
  argTypes: {
    state: {
      control: "select",
      options: STATES,
      description: "Which branch of the flow renders: bid list, casting spinner, error, redirect.",
    },
    vote: { control: "text", description: "The choice the voter is casting." },
    retryTimer: { control: "text", description: "Countdown shown on the error action." },
    bids: { control: "object", description: "The competing bids listed under the vote." },
  },
  args: { bids: BIDS, vote: "Yes", state: "default", retryTimer: "30s" },
} satisfies Meta<typeof GvBidVotingFlow>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const Catalog: Story = {
  name: "Catalog (every state)",
  parameters: {
    controls: { disable: true },
  },
  render: () => (
    <div className="gv" style={{ display: "flex", flexDirection: "column", gap: 48 }}>
      {CASES.map((props, i) => (
        // <section> demotes each entry's unnamed header/footer/aside to `generic`
        <section key={i}>
          <GvBidVotingFlow bids={BIDS} vote="Yes" {...props} chrome={false} />
        </section>
      ))}
    </div>
  ),
};
