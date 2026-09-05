import type { Meta, StoryObj } from "@storybook/react-vite";
import type { CSSProperties } from "react";
import { OrientationProvider } from "./OrientationProvider";
import OrientationGate from "./OrientationGate";

const STAGE: CSSProperties = {
  position: "fixed",
  inset: 0,
  display: "flex",
  flexDirection: "column",
  alignItems: "center",
  justifyContent: "center",
  gap: 12,
  background: "linear-gradient(160deg, #2a2433 0%, #15121b 100%)",
  color: "#fff",
  fontFamily: "var(--font-sans)",
  fontSize: 13,
};

const CHIP: CSSProperties = {
  padding: "10px 18px",
  borderRadius: 999,
  background: "rgba(255, 45, 85, .18)",
  border: "1px solid rgba(255, 45, 85, .5)",
};

const meta = {
  title: "Explorer/Mobile/Layout/OrientationGate",
  component: OrientationGate,
  parameters: { layout: "fullscreen" },
  decorators: [
    (Story) => (
      <OrientationProvider>
        <div style={STAGE}>
          <Story />
        </div>
      </OrientationProvider>
    ),
  ],
} satisfies Meta<typeof OrientationGate>;

export default meta;
type Story = StoryObj<typeof meta>;

export const LandscapeOnly: Story = {
  args: {
    show: "landscape",
    children: <div style={CHIP}>landscape only &#x2014; unmounted in portrait</div>,
  },
};

export const PortraitOnly: Story = {
  args: {
    show: "portrait",
    children: <div style={CHIP}>portrait only &#x2014; unmounted in landscape</div>,
  },
};

export const HideModeLandscape: Story = {
  args: {
    show: "landscape",
    mode: "hide",
    children: <div style={CHIP}>landscape only &#x2014; stays mounted, display:none in portrait</div>,
  },
};
