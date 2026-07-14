import type { Meta, StoryObj } from "@storybook/react-vite";
import SkyboxHUD from "./SkyboxHUD";

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
