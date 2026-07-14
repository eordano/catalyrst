import type { Meta, StoryObj } from "@storybook/react-vite";
import MobileActionCluster from "./MobileActionCluster";
import StoryWorld from "./StoryWorld";

const meta = {
  title: "Explorer/Mobile/ActionCluster",
  component: MobileActionCluster,
  parameters: {
    layout: "fullscreen",
    docs: {
      description: {
        component:
          "Bottom-right action cluster \u{2014} the port of godot's Joypad. Buttons are hold buttons: " +
          "`onActionPress` fires on pointerdown and `onActionRelease` on pointerup, pointercancel, " +
          "lost capture and unmount, so a gesture can never latch. The cluster is presentational; " +
          "the touch-controls lane turns those callbacks into synthesized key events at the canvas.",
      },
    },
  },
  args: {
    onActionPress: () => {},
    onActionRelease: () => {},
  },
  decorators: [
    (Story) => (
      <StoryWorld>
        <Story />
      </StoryWorld>
    ),
  ],
} satisfies Meta<typeof MobileActionCluster>;

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

export const JumpOnly: Story = {
  args: {
    orientation: "landscape",
    actions: [{ id: "jump", label: "Jump", glyph: "jump" }],
  },
  globals: { viewport: { value: "mobile", isRotated: true } },
};
