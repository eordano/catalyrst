import type { Meta, StoryObj } from "@storybook/react-vite";
import GvProfileActivity from "./GvProfileActivity";

const meta = {
  title: "Governance/Pages/Profile / Activity",
  component: GvProfileActivity,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof GvProfileActivity>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const MemberProfile: Story = {
  args: {
    username: "luna.dcl",
    address: "0x3e…c901",
    isOwnProfile: false,
    bio: "Wearable creator and frequent voter. Delegating to community stewards.",
  },
};

export const NoBio: Story = {
  args: { bio: "" },
};
