import type { Meta, StoryObj } from "@storybook/react-vite";
import MobileTopBar from "./MobileTopBar";
import StoryWorld from "./StoryWorld";

const meta = {
  title: "Explorer/Mobile/TopBar",
  component: MobileTopBar,
  parameters: {
    layout: "fullscreen",
    docs: {
      description: {
        component:
          "In-world top strip for touch layouts. Portrait draws a full-width bar under the " +
          "status area; landscape floats the same controls as pills inset from the left/right " +
          "safe edges. Padding comes from `--safe-*` with an `env(safe-area-inset-*)` fallback, " +
          "so it is inert until the page declares `viewport-fit=cover`.",
      },
    },
  },
  args: {
    place: "Genesis Plaza",
    coords: "0,0",
    onMenu: () => {},
    onChat: () => {},
    onHideHud: () => {},
  },
  decorators: [
    (Story) => (
      <StoryWorld>
        <Story />
      </StoryWorld>
    ),
  ],
} satisfies Meta<typeof MobileTopBar>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Portrait: Story = {
  args: { orientation: "portrait", unread: 3 },
  globals: { viewport: { value: "mobile", isRotated: false } },
};

export const Landscape: Story = {
  args: { orientation: "landscape" },
  globals: { viewport: { value: "mobile", isRotated: true } },
};

export const EmptyLocation: Story = {
  args: { orientation: "portrait", place: "", coords: "" },
  globals: { viewport: { value: "mobile", isRotated: false } },
};
