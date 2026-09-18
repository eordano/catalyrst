import type { Meta, StoryObj } from "@storybook/react-vite";
import SkyboxHUD, { SkyboxControls } from "./SkyboxHUD";
import FloatingPanel from "./FloatingPanel";
import { HudPanelScene } from "./FloatingPanel.stories";

const meta = {
  title: "Explorer/Components/SkyboxHUD",
  component: SkyboxHUD,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof SkyboxHUD>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => (
    <div style={{ minHeight: "100vh", background: "#1a1a1f" }}>
      <SkyboxHUD />
    </div>
  ),
};

export const Floating: Story = {
  render: () => (
    <HudPanelScene panel="skybox">
      <FloatingPanel id="skybox" onClose={() => {}}>
        <SkyboxControls />
      </FloatingPanel>
    </HudPanelScene>
  ),
};
