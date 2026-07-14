import type { Meta, StoryObj } from "@storybook/react-vite";
import Camera from "./Camera";

const meta = {
  title: "Explorer/Pages/Camera",
  component: Camera,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof Camera>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => <Camera />,
};
