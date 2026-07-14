import type { Meta, StoryObj } from "@storybook/react-vite";
import Communities from "./Communities";

const meta = {
  title: "Explorer/Pages/Communities",
  component: Communities,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof Communities>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => <Communities />,
};
