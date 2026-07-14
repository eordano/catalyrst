import type { Meta, StoryObj } from "@storybook/react-vite";
import VoiceChat from "./VoiceChat";

const meta = {
  title: "Explorer/Components/VoiceChat",
  component: VoiceChat,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof VoiceChat>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => <VoiceChat bare />,
};
