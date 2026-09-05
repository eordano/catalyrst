import type { Meta, StoryObj } from "@storybook/react-vite";
import CommunityMembers from "./CommunityMembers";

const meta = {
  title: "Explorer/Pages/CommunityMembers",
  component: CommunityMembers,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof CommunityMembers>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => <CommunityMembers />,
};
