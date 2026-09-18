import type { Meta, StoryObj } from "@storybook/react-vite";
import Minimap from "./Minimap";
import { MinimapVisibilityProvider } from "../../overlay/minimapVisibility";

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

export const UserHidden: Story = {
  args: { place: "Genesis Plaza", coords: "0,0" },
  decorators: [
    (StoryFn) => {
      try {
        localStorage.setItem("dcl.minimap.userHidden", "1");
      } catch {
      }
      return (
        <MinimapVisibilityProvider>
          <StoryFn />
        </MinimapVisibilityProvider>
      );
    },
  ],
};
