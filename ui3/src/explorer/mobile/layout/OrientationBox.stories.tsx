import type { Meta, StoryObj } from "@storybook/react-vite";
import type { CSSProperties } from "react";
import { OrientationProvider } from "./OrientationProvider";
import OrientationBox from "./OrientationBox";

const STAGE: CSSProperties = {
  position: "fixed",
  inset: 0,
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  background: "linear-gradient(160deg, #2a2433 0%, #15121b 100%)",
  color: "#fff",
  fontFamily: "var(--font-sans)",
  fontSize: 13,
};

const TILE: CSSProperties = {
  minWidth: 120,
  minHeight: 68,
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  borderRadius: 12,
  background: "rgba(255, 255, 255, .08)",
  border: "1px solid rgba(255, 255, 255, .14)",
};

const tiles = ["one", "two", "three"];

const meta = {
  title: "Explorer/Mobile/Layout/OrientationBox",
  component: OrientationBox,
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
} satisfies Meta<typeof OrientationBox>;

export default meta;
type Story = StoryObj<typeof meta>;

export const RowInLandscape: Story = {
  args: {
    gap: 24,
    children: tiles.map((t) => (
      <div key={t} style={TILE}>
        {t}
      </div>
    )),
  },
};

export const Inverted: Story = {
  args: {
    invert: true,
    gap: 24,
    children: tiles.map((t) => (
      <div key={t} style={TILE}>
        {t}
      </div>
    )),
  },
};
