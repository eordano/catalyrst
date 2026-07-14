import type { Meta, StoryObj } from "@storybook/react-vite";
import Minimap from "./Minimap";

const meta = {
  title: "Explorer/Frames/Minimap",
  component: Minimap,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof Minimap>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  args: { place: "Genesis Plaza", coords: "0,0" },
};

export const NamedScene: Story = {
  args: { place: "Wonderzone Meteorchaser", coords: "-58,124" },
};
