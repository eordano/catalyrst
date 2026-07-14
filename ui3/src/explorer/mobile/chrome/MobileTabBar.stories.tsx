import type { Meta, StoryObj } from "@storybook/react-vite";
import MobileTabBar from "./MobileTabBar";
import StoryWorld from "./StoryWorld";

const meta = {
  title: "Explorer/Mobile/TabBar",
  component: MobileTabBar,
  parameters: {
    layout: "fullscreen",
    docs: {
      description: {
        component:
          "Portrait bottom tab bar — the port of godot's `BottomBar_Portrait`, which is destroyed " +
          "in landscape. Here landscape unmounts it by default (`landscape=\"hidden\"`); the " +
          "`landscape=\"pill\"` variant exists for layouts that keep tab navigation while the " +
          "right rail is unavailable.",
      },
    },
  },
  args: { active: "places", onTab: () => {} },
  decorators: [
    (Story) => (
      <StoryWorld>
        <Story />
      </StoryWorld>
    ),
  ],
} satisfies Meta<typeof MobileTabBar>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Portrait: Story = {
  args: { orientation: "portrait" },
  globals: { viewport: { value: "mobile", isRotated: false } },
};

export const Landscape: Story = {
  args: { orientation: "landscape", landscape: "pill" },
  globals: { viewport: { value: "mobile", isRotated: true } },
};

export const LandscapeHidden: Story = {
  args: { orientation: "landscape" },
  globals: { viewport: { value: "mobile", isRotated: true } },
};
