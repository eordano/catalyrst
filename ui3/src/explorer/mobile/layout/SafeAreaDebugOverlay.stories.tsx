import type { Meta, StoryObj } from "@storybook/react-vite";
import type { CSSProperties } from "react";
import { OrientationProvider } from "./OrientationProvider";
import SafeAreaDebugOverlay from "./SafeAreaDebugOverlay";

const STAGE: CSSProperties = {
  position: "fixed",
  inset: 0,
  background: "repeating-linear-gradient(135deg, #241f2c 0 12px, #1a1622 12px 24px)",
};

const meta = {
  title: "Explorer/Mobile/Layout/SafeAreaDebugOverlay",
  component: SafeAreaDebugOverlay,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof SafeAreaDebugOverlay>;

export default meta;
type Story = StoryObj<typeof meta>;

export const EmulatedIos: Story = {
  args: {},
  decorators: [
    (Story) => (
      <OrientationProvider safeAreaEmulation="ios">
        <div style={STAGE} />
        <Story />
      </OrientationProvider>
    ),
  ],
};

export const EmulatedAndroid: Story = {
  args: {},
  decorators: [
    (Story) => (
      <OrientationProvider safeAreaEmulation="android">
        <div style={STAGE} />
        <Story />
      </OrientationProvider>
    ),
  ],
};

export const RealInsets: Story = {
  args: {},
  decorators: [
    (Story) => (
      <OrientationProvider>
        <div style={STAGE} />
        <Story />
      </OrientationProvider>
    ),
  ],
};
