import type { Meta, StoryObj } from "@storybook/react-vite";
import GvSubmitProjectUpdate from "./GvSubmitProjectUpdate";

const meta = {
  title: "Governance/Pages/Submit Project Update",
  component: GvSubmitProjectUpdate,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof GvSubmitProjectUpdate>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const Filled: Story = {
  args: {
    funds: {
      released: "42,500",
      disclosed: "0",
      token: "USD",
      releasedNote: "3 tx. last one made 12 days ago",
      undisclosedNote: "42,500 USD left undisclosed",
    },
    initialValues: {
      introduction:
        "This update covers our second development sprint, focused on the marketplace integration milestone.",
      highlights:
        "- Shipped the on-chain settlement flow\n- Closed 14 issues from the backlog\n- Onboarded two new contributors",
      blockers:
        "Waiting on the indexer migration before we can finalize the analytics dashboard. Mitigation: parallelizing the front-end work in the meantime.",
      next_steps:
        "Finalize the analytics dashboard, ship the public beta, and begin the security audit.",
      additional_notes: "Demo build available on our staging environment.",
    },
  },
};

export const SignedOut: Story = {
  args: { signedIn: false },
};
