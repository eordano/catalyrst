import type { Meta, StoryObj } from "@storybook/react-vite";
import EngineToasts from "./EngineToasts";

const meta = {
  title: "Explorer/Components/EngineToasts",
  component: EngineToasts,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof EngineToasts>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  args: {
    toasts: [
      { key: "scene-error", message: "Scene 74,-12 hit a runtime error and was restarted" },
      { key: "realm", message: "Connected to realm hela" },
    ],
  },
  render: (args) => (
    <div style={{ position: "relative", minHeight: "100vh", background: "#1a1a1f" }}>
      <EngineToasts {...args} />
    </div>
  ),
};

export const Empty: Story = {
  args: { toasts: [] },
  render: (args) => (
    <div style={{ position: "relative", minHeight: "100vh", background: "#1a1a1f" }}>
      <EngineToasts {...args} />
    </div>
  ),
};
