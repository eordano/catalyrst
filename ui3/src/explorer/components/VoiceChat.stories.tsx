import type { Meta, StoryObj } from "@storybook/react-vite";
import VoiceChat, { VoiceControls } from "./VoiceChat";
import FloatingPanel from "./FloatingPanel";
import { HudPanelScene } from "./FloatingPanel.stories";

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

export const Floating: Story = {
  render: () => (
    <HudPanelScene panel="voice">
      <FloatingPanel id="voice" onClose={() => {}}>
        <VoiceControls />
      </FloatingPanel>
    </HudPanelScene>
  ),
};
