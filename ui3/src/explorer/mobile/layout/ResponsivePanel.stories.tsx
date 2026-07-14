import type { Meta, StoryObj } from "@storybook/react-vite";
import type { CSSProperties } from "react";
import { OrientationProvider } from "./OrientationProvider";
import ResponsivePanel from "./ResponsivePanel";

const STAGE: CSSProperties = {
  position: "fixed",
  inset: 0,
  background: "linear-gradient(160deg, #2a2433 0%, #15121b 100%)",
};

const PANEL: CSSProperties = {
  borderRadius: 18,
  background: "rgba(22, 21, 24, .94)",
  border: "1px solid rgba(255, 255, 255, .08)",
  color: "#fff",
  fontFamily: "var(--font-sans)",
  fontSize: 13,
  padding: 22,
  boxShadow: "0 30px 80px rgba(0, 0, 0, .5)",
};

const meta = {
  title: "Explorer/Mobile/Layout/ResponsivePanel",
  component: ResponsivePanel,
  parameters: { layout: "fullscreen" },
  decorators: [
    (Story) => (
      <OrientationProvider>
        <div style={STAGE} />
        <Story />
      </OrientationProvider>
    ),
  ],
} satisfies Meta<typeof ResponsivePanel>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Defaults: Story = {
  args: {
    style: PANEL,
    children: <div>0.90 wide in portrait, 0.45 in landscape, max 0.80 tall.</div>,
  },
};

export const ModalPreset: Story = {
  args: {
    preset: "modal",
    style: PANEL,
    children: <div>modal preset — portrait 0.72 x 0.10-0.80, landscape 0.45 x 0.10-0.80.</div>,
  },
};

export const TravelModalPreset: Story = {
  args: {
    preset: "travelModal",
    style: PANEL,
    children: <div>travelModal preset — portrait 0.85 x 0.40, landscape 0.55 x 0.50.</div>,
  },
};

export const TopAligned: Story = {
  args: {
    preset: "codeModal",
    centerVertical: false,
    verticalOffset: 48,
    style: PANEL,
    children: <div>centerVertical off, verticalOffset 48 design px.</div>,
  },
};
