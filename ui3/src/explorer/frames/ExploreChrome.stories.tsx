import type { Meta, StoryObj } from "@storybook/react-vite";
import ExploreChrome from "./ExploreChrome";

const meta = {
  title: "Explorer/Frames/ExploreChrome",
  component: ExploreChrome,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof ExploreChrome>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  args: {
    active: "events",
    user: "Evaristo",
    children: (
      <div style={{ padding: 32, color: "#fff", fontSize: 18 }}>
        Active screen content renders here.
      </div>
    ),
  },
};

export const PlacesActive: Story = {
  args: {
    ...Default.args,
    active: "places",
  },
};

export const BackpackActive: Story = {
  args: {
    ...Default.args,
    active: "backpack",
  },
};
