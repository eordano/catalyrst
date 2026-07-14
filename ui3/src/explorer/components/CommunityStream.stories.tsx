import type { Meta, StoryObj } from "@storybook/react-vite";
import CommunityStream from "./CommunityStream";

const meta = {
  title: "Explorer/Components/CommunityStream",
  component: CommunityStream,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof CommunityStream>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => <CommunityStream />,
};
