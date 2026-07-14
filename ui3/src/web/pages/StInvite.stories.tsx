import type { Meta, StoryObj } from "@storybook/react-vite";
import StInvite from "./StInvite";

const meta = {
  title: "Web/Pages/Invite",
  component: StInvite,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof StInvite>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  args: {
    referrer: {
      name: "MetaPioneer",
      ethAddress: "0x8a3b2f19c4e7d05a6b1f9e2c3d4a5b6c7d8e9f01",
    },
    loading: false,
  },
};

export const Loading: Story = {
  args: {
    referrer: {
      name: "MetaPioneer",
      ethAddress: "0x8a3b2f19c4e7d05a6b1f9e2c3d4a5b6c7d8e9f01",
    },
    loading: true,
  },
};

export const NoReferrer: Story = {
  args: {
    referrer: null,
    loading: false,
  },
};
