import type { Meta, StoryObj } from "@storybook/react-vite";
import CommunityCreate from "./CommunityCreate";

const meta = {
  title: "Explorer/Components/CommunityCreate",
  component: CommunityCreate,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof CommunityCreate>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => <CommunityCreate standalone />,
};
