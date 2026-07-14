import type { Meta, StoryObj } from "@storybook/react-vite";
import type { CSSProperties, ReactNode } from "react";
import { OrientationProvider } from "./OrientationProvider";
import SafeArea from "./SafeArea";

const STAGE: CSSProperties = {
  position: "fixed",
  inset: 0,
  background: "repeating-linear-gradient(135deg, #241f2c 0 12px, #1a1622 12px 24px)",
  color: "#fff",
  fontFamily: "var(--font-sans)",
};

const CONTENT: CSSProperties = {
  height: "100%",
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  borderRadius: 12,
  background: "rgba(255, 45, 85, .18)",
  border: "1px solid rgba(255, 45, 85, .5)",
  fontSize: 13,
};

function Stage({ children }: { children: ReactNode }) {
  return <div style={STAGE}>{children}</div>;
}

const meta = {
  title: "Explorer/Mobile/Layout/SafeArea",
  component: SafeArea,
  parameters: { layout: "fullscreen" },
  decorators: [
    (Story) => (
      <OrientationProvider safeAreaEmulation="ios">
        <Stage>
          <Story />
        </Stage>
      </OrientationProvider>
    ),
  ],
} satisfies Meta<typeof SafeArea>;

export default meta;
type Story = StoryObj<typeof meta>;

export const AllEdges: Story = {
  args: {
    style: { height: "100%" },
    children: <div style={CONTENT}>all four edges inset</div>,
  },
};

export const LeftRightOnly: Story = {
  args: {
    edges: { top: false, bottom: false },
    style: { height: "100%" },
    children: <div style={CONTENT}>left / right only</div>,
  },
};

export const WithDefaultMargin: Story = {
  args: {
    defaultMargin: 48,
    style: { height: "100%" },
    children: <div style={CONTENT}>defaultMargin 48 design px, floored by the inset</div>,
  },
};

export const WithMinMargins: Story = {
  args: {
    edges: { bottom: false },
    minMargin: { top: 110, right: 12 },
    style: { height: "100%" },
    children: <div style={CONTENT}>min margins top 110 / right 12</div>,
  },
};
