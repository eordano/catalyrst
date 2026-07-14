import type { Meta, StoryObj } from "@storybook/react-vite";
import TeleportPrompt from "./TeleportPrompt";

const meta = {
  title: "Explorer/Components/TeleportPrompt",
  component: TeleportPrompt,
  parameters: { layout: "fullscreen" },
  tags: ["overlay"],
} satisfies Meta<typeof TeleportPrompt>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => <TeleportPrompt />,
};
