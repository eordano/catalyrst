import type { Meta, StoryObj } from "@storybook/react-vite";
import GvProjectsList from "./GvProjectsList";

const meta = {
  title: "Governance/Pages/Projects List",
  component: GvProjectsList,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof GvProjectsList>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const Empty: Story = {
  args: { projects: [] },
};
