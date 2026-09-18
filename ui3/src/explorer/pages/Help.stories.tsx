import type { Meta, StoryObj } from "@storybook/react-vite";
import ExploreChrome from "../frames/ExploreChrome";
import Help from "./Help";

const meta = {
  title: "Explorer/Pages/Help",
  component: Help,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof Help>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => (
    <ExploreChrome user="CosmicLux">
      <Help />
    </ExploreChrome>
  ),
};
