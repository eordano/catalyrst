import type { Meta, StoryObj } from "@storybook/react-vite";
import ChLearn from "./ChLearn";

const meta = {
  title: "CreatorHub/Pages/Learn",
  component: ChLearn,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof ChLearn>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};
