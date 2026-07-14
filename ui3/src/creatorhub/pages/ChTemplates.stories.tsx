import type { Meta, StoryObj } from "@storybook/react-vite";
import ChTemplates from "./ChTemplates";

const meta = {
  title: "CreatorHub/Pages/Templates",
  component: ChTemplates,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof ChTemplates>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const Empty: Story = {
  args: { templates: [] },
};

export const CreateProjectModal: Story = {
  args: { modalOpen: true },
};
