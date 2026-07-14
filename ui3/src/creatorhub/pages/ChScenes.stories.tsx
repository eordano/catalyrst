import type { Meta, StoryObj } from "@storybook/react-vite";
import ChScenes from "./ChScenes";

const meta = {
  title: "CreatorHub/Pages/Scenes",
  component: ChScenes,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof ChScenes>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  args: { state: "default" },
};

export const Empty: Story = {
  args: { state: "empty" },
};

export const Loading: Story = {
  args: { state: "loading" },
};
