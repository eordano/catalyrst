import type { Meta, StoryObj } from "@storybook/react-vite";
import type { CSSProperties } from "react";
import { OrientationProvider } from "./OrientationProvider";
import FigmaBox, { FIGMA_BOX_MODAL } from "./FigmaBox";

const STAGE: CSSProperties = {
  position: "fixed",
  inset: 0,
  background: "linear-gradient(160deg, #2a2433 0%, #15121b 100%)",
};

const FRAME: CSSProperties = {
  position: "fixed",
  inset: "8vh 8vw",
  borderRadius: 18,
  background: "rgba(22, 21, 24, .94)",
  border: "1px solid rgba(255, 255, 255, .08)",
  color: "#fff",
  fontFamily: "var(--font-sans)",
  fontSize: 13,
};

const INNER: CSSProperties = {
  height: "100%",
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  borderRadius: 10,
  background: "rgba(255, 45, 85, .18)",
  border: "1px solid rgba(255, 45, 85, .5)",
  textAlign: "center",
};

const meta = {
  title: "Explorer/Mobile/Layout/FigmaBox",
  component: FigmaBox,
  parameters: { layout: "fullscreen" },
  decorators: [
    (Story) => (
      <OrientationProvider>
        <div style={STAGE} />
        <Story />
      </OrientationProvider>
    ),
  ],
} satisfies Meta<typeof FigmaBox>;

export default meta;
type Story = StoryObj<typeof meta>;

export const ModalContent: Story = {
  args: {
    ...FIGMA_BOX_MODAL,
    style: FRAME,
    children: (
      <div style={INNER}>
        modal content box &#x2014; 64/56/60/56 against a 1600 portrait frame, 80/80/70/80 against a 720
        landscape frame
      </div>
    ),
  },
};

export const Defaults: Story = {
  args: {
    style: FRAME,
    children: <div style={INNER}>20 design px on every edge of a 720-tall frame</div>,
  },
};
