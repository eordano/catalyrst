import type { Meta, StoryObj } from "@storybook/react-vite";
import ChScenesEmptyState from "./ChScenesEmptyState";

const meta = {
  title: "CreatorHub/Pages/Scenes empty state",
  component: ChScenesEmptyState,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof ChScenesEmptyState>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};
