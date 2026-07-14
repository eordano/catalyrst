import type { Meta, StoryObj } from "@storybook/react-vite";
import Settings from "./Settings";

const meta = {
  title: "Explorer/Pages/Settings",
  component: Settings,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof Settings>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => <Settings />,
};
