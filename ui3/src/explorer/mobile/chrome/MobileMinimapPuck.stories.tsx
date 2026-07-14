import type { Meta, StoryObj } from "@storybook/react-vite";
import MobileMinimapPuck from "./MobileMinimapPuck";
import StoryWorld from "./StoryWorld";

const meta = {
  title: "Explorer/Mobile/MinimapPuck",
  component: MobileMinimapPuck,
  parameters: {
    layout: "fullscreen",
    docs: {
      description: {
        component:
          "Touch treatment of the minimap. Landscape keeps a round map puck with the heading " +
          "arrow; portrait collapses to a coordinate chip, since godot mobile has no minimap at " +
          "all and portrait is the browse mode. The map image is supplied by the container " +
          "(`mapSrc`); with none, only the parcel grid is drawn.",
      },
    },
  },
  args: {
    place: "Genesis Plaza",
    coords: "0,0",
    heading: 34,
    onExpand: () => {},
    onToggleCollapsed: () => {},
  },
  decorators: [
    (Story) => (
      <StoryWorld>
        <Story />
      </StoryWorld>
    ),
  ],
} satisfies Meta<typeof MobileMinimapPuck>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Portrait: Story = {
  args: { orientation: "portrait" },
  globals: { viewport: { value: "mobile", isRotated: false } },
};

export const Landscape: Story = {
  args: { orientation: "landscape" },
  globals: { viewport: { value: "mobile", isRotated: true } },
};

export const LandscapeCollapsed: Story = {
  args: { orientation: "landscape", collapsed: true },
  globals: { viewport: { value: "mobile", isRotated: true } },
};
