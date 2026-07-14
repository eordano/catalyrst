import type { Meta, StoryObj } from "@storybook/react-vite";
import { useState } from "react";
import type { MobileOrientation, MobileSheetSnap } from "./types";
import MobileSheet from "./MobileSheet";
import StoryWorld from "./StoryWorld";

const meta = {
  title: "Explorer/Mobile/Sheet",
  component: MobileSheet,
  parameters: {
    layout: "fullscreen",
    docs: {
      description: {
        component:
          "Mobile presentation of a detail panel \u{2014} the port of godot's `SidePanelWrapper`. " +
          "Portrait is a draggable bottom sheet (peek 64px / half / full, 50px drag threshold, " +
          "0.2s snap); landscape is a right-edge drawer. Tapping the scrim closes. The panel body " +
          "is whatever the container passes as children.",
      },
    },
  },
  args: { title: "Place details", onClose: () => {} },
  decorators: [
    (Story) => (
      <StoryWorld>
        <Story />
      </StoryWorld>
    ),
  ],
} satisfies Meta<typeof MobileSheet>;

export default meta;
type Story = StoryObj<typeof meta>;

function Body() {
  return (
    <p style={{ margin: 0, fontSize: 13, color: "var(--ink-7)" }}>
      The container owns this panel body. Drag the handle to move between peek, half and full.
    </p>
  );
}

function Demo({ orientation, snap }: { orientation: MobileOrientation; snap: MobileSheetSnap }) {
  const [open, setOpen] = useState(true);
  return (
    <MobileSheet
      open={open}
      orientation={orientation}
      title="Place details"
      defaultSnap={snap}
      onClose={() => setOpen(false)}
    >
      <Body />
    </MobileSheet>
  );
}

export const Portrait: Story = {
  render: () => <Demo orientation="portrait" snap="half" />,
  globals: { viewport: { value: "mobile", isRotated: false } },
};

export const PortraitFull: Story = {
  render: () => <Demo orientation="portrait" snap="full" />,
  globals: { viewport: { value: "mobile", isRotated: false } },
};

export const Landscape: Story = {
  render: () => <Demo orientation="landscape" snap="full" />,
  globals: { viewport: { value: "mobile", isRotated: true } },
};
