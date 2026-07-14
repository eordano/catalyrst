import type { Meta, StoryObj } from "@storybook/react-vite";
import GvSubmitPoll from "./GvSubmitPoll";

const meta = {
  title: "Governance/Pages/Submit Poll",
  component: GvSubmitPoll,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof GvSubmitPoll>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  args: { loggedIn: true, vpMet: true },
};

export const LogInGate: Story = {
  args: { loggedIn: false },
};

export const VpNotMet: Story = {
  args: { loggedIn: true, vpMet: false },
};
