import type { Meta, StoryObj } from "@storybook/react-vite";
import type { CSSProperties } from "react";
import { OrientationProvider, useOrientationState } from "./OrientationProvider";

const STAGE: CSSProperties = {
  width: "100vw",
  height: "100vh",
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  background: "linear-gradient(160deg, #2a2433 0%, #15121b 100%)",
  color: "#fff",
  fontFamily: "var(--font-sans)",
};

const CARD: CSSProperties = {
  minWidth: 260,
  padding: "18px 20px",
  borderRadius: 14,
  background: "rgba(14, 12, 18, .72)",
  boxShadow: "0 8px 28px rgba(0, 0, 0, .4)",
  fontSize: 13,
  lineHeight: 1.8,
};

const ROW: CSSProperties = { display: "flex", justifyContent: "space-between", gap: 18 };
const KEY: CSSProperties = { color: "rgba(255, 255, 255, .6)" };

function Readout() {
  const state = useOrientationState();
  return (
    <div style={STAGE}>
      <div style={CARD}>
        <div style={ROW}>
          <span style={KEY}>resolved</span>
          <b>{state.orientation}</b>
        </div>
        <div style={ROW}>
          <span style={KEY}>viewport</span>
          <span>{state.viewport}</span>
        </div>
        <div style={ROW}>
          <span style={KEY}>declared</span>
          <span>{state.declared ?? "none"}</span>
        </div>
      </div>
    </div>
  );
}

const meta = {
  title: "Explorer/Mobile/Layout/OrientationProvider",
  component: OrientationProvider,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof OrientationProvider>;

export default meta;
type Story = StoryObj<typeof meta>;

export const FollowsViewport: Story = {
  args: { children: <Readout /> },
};

export const DeclaredLandscape: Story = {
  args: { orientation: "landscape", children: <Readout /> },
};

export const DeclaredPortrait: Story = {
  args: { orientation: "portrait", children: <Readout /> },
};
