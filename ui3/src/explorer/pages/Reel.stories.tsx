import type { Meta, StoryObj } from "@storybook/react-vite";
import Reel from "./Reel";

const meta = {
  title: "Explorer/Pages/Reel",
  component: Reel,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof Reel>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => <Reel />,
};
