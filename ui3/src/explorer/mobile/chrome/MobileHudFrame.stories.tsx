import type { Meta, StoryObj } from "@storybook/react-vite";
import MobileHudFrame from "./MobileHudFrame";
import StoryWorld from "./StoryWorld";

const meta = {
  title: "Explorer/Mobile/HudFrame",
  component: MobileHudFrame,
  parameters: {
    layout: "fullscreen",
    docs: {
      description: {
        component:
          "Composition of the touch HUD: top bar, minimap treatment, action cluster, portrait " +
          "tab bar and crosshair. `controlsSlot` renders in its own layer beneath both this HUD " +
          "and the desktop overlay, so the joystick/look catcher never steals a tap from an " +
          "interactive widget. Landscape is the play mode (no tab bar, crosshair " +
          "on); portrait is the browse mode (tab bar, collapsed map chip).",
      },
    },
  },
  args: {
    place: "Genesis Plaza",
    coords: "0,0",
    heading: 34,
    onMenu: () => {},
    onChat: () => {},
    onTab: () => {},
    onExpandMap: () => {},
    onToggleMap: () => {},
    onHideHud: () => {},
    onShowHud: () => {},
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
} satisfies Meta<typeof MobileHudFrame>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Portrait: Story = {
  args: { orientation: "portrait", activeTab: "places", unread: 3 },
  globals: { viewport: { value: "mobile", isRotated: false } },
};

export const Landscape: Story = {
  args: { orientation: "landscape", crosshair: true },
  globals: { viewport: { value: "mobile", isRotated: true } },
};

export const LandscapeHudHidden: Story = {
  args: { orientation: "landscape", hudHidden: true },
  globals: { viewport: { value: "mobile", isRotated: true } },
};
