import type { Meta, StoryObj } from "@storybook/react-vite";
import GvSubmitBid from "./GvSubmitBid";

const meta = {
  title: "Governance/Pages/Submit Bid",
  component: GvSubmitBid,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof GvSubmitBid>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const Submitted: Story = {
  args: { submitted: true },
};
