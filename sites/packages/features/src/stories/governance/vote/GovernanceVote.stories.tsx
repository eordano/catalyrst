import type { Meta, StoryObj } from "@ui/docs/sb";
import { userEvent } from "@ui/docs/sb";

import GovernanceVote from "./GovernanceVote";
import { simulateCast } from "./machine";

const meta = {
  title: "Sites Specs/governance/vote/GovernanceVote",
  component: GovernanceVote,
  parameters: {
    layout: "padded",
    a11y: { test: "todo" },
  },
  args: {
    proposalId: "prop-0001",
    variant: "modal",
    flags: {},
    trackCtx: { sid: "sb-spec", story: "governance/vote", variant: "modal" },
    castVote: simulateCast,
    track: () => {},
  },
} satisfies Meta<typeof GovernanceVote>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Choosing: Story = {};

export const GuidedReasoning: Story = {
  args: { flags: { guided: true } },
  play: async ({ canvas }) => {
    await userEvent.click(await canvas.findByRole("button", { name: "Cast Vote" }));
    await canvas.findByText("Please explain the reasoning behind your vote");
  },
};

export const HappyPath: Story = {
  play: async ({ canvas }) => {
    await userEvent.click(await canvas.findByRole("button", { name: "Cast Vote" }));
    await canvas.findByText("Vote Registered");
    await userEvent.click(await canvas.findByRole("button", { name: "Close" }));
    await canvas.findByText(/has been recorded/);
  },
};

export const SnapshotFallback: Story = {
  args: {
    flags: { guided: true },
    castVote: async () => {
      throw new Error("snapshot relay 502");
    },
  },
  play: async ({ canvas }) => {
    await userEvent.click(await canvas.findByRole("button", { name: "Cast Vote" }));
    await canvas.findByText("Please explain the reasoning behind your vote");
    await userEvent.click(await canvas.findByRole("button", { name: "Cast Vote" }));
    await userEvent.click(await canvas.findByRole("button", { name: "Retry" }));
    await userEvent.click(await canvas.findByRole("button", { name: "Retry" }));
    await canvas.findByText("Voting not available");
    await canvas.findByRole("link", { name: /Vote on Snapshot/ });
  },
};
