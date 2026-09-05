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

/** Every branch of the flow's `state` switch. The prop is a bare `string`, so enumerate it here. */
const STATES = ["default", "casting", "error", "redirect"] as const;

/** Every state, in the order the variant stories used to declare them. */
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

/**
 * Every state rendered at once. `Default` flips between them with the Controls panel; this story
 * keeps all four in the render + a11y + visual-diff gates, since the bid list, the casting spinner,
 * the error action and the Snapshot redirect are structurally different subtrees.
 */
export const Catalog: Story = {
  name: "Catalog (every state)",
  parameters: {
    controls: { disable: true },
  },
  render: () => (
    <div className="gv" style={{ display: "flex", flexDirection: "column", gap: 48 }}>
      {CASES.map((props, i) => (
        // <section> demotes each entry's unnamed header/footer/aside to `generic`
        // (HTML-AAM scoped mapping) so the stack does not invent extra landmarks.
        <section key={i}>
          <GvBidVotingFlow bids={BIDS} vote="Yes" {...props} chrome={false} />
        </section>
      ))}
    </div>
  ),
};
