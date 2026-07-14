import type { Meta, StoryObj } from "@storybook/react-vite";
import type { ReactNode } from "react";
import { useState } from "react";
import Notifications from "./Notifications";
import Minimap from "../frames/Minimap";
import { MinimapVisibilityProvider } from "../../overlay/minimapVisibility";
import "../../overlay/overlay.css";

const meta = {
  title: "Explorer/Components/Notifications",
  component: Notifications,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof Notifications>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => <Notifications />,
};

export const Bare: Story = {
  parameters: { layout: "centered", sceneBackdrop: false },
  render: () => <Notifications bare />,
};

const WORLD =
  "radial-gradient(120% 90% at 30% 10%, #3a2f5c 0%, #1c1830 45%, #0c0a14 100%)";

function HudScene({ notifOpen, children }: { notifOpen: boolean; children?: ReactNode }) {
  return (
    <div style={{ position: "fixed", inset: 0, background: WORLD }}>
      <MinimapVisibilityProvider>
        <div className="ui3-overlay" data-live="true">
          <div className="ui3-overlay__widget ui3-overlay__minimap">
            <Minimap place="Genesis Plaza" coords="0,0" />
          </div>
          {notifOpen && (
            <div className="ui3-overlay__widget ui3-overlay__notifications">
              <Notifications bare floating />
            </div>
          )}
          {children}
        </div>
      </MinimapVisibilityProvider>
    </div>
  );
}

export const ClosedMinimapVisible: Story = {
  name: "Closed (minimap visible)",
  render: () => <HudScene notifOpen={false} />,
};

export const OpenHalfTransparentMinimapHidden: Story = {
  name: "Open (half-transparent · minimap hidden)",
  render: () => <HudScene notifOpen />,
};

function ToggleDemo() {
  const [open, setOpen] = useState(false);
  return (
    <HudScene notifOpen={open}>
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        style={{
          position: "absolute",
          left: 16,
          bottom: 16,
          zIndex: 50,
          pointerEvents: "auto",
          padding: "10px 16px",
          borderRadius: 10,
          border: "1px solid rgba(255,255,255,.18)",
          background: open ? "var(--brand, #ff2d55)" : "rgba(20,18,28,.92)",
          color: "#fff",
          font: "inherit",
          fontWeight: 700,
          cursor: "pointer",
        }}
      >
        {open ? "Close notifications (restore minimap)" : "Open notifications"}
      </button>
    </HudScene>
  );
}

export const Toggle: Story = {
  name: "Toggle (open ↔ close restores minimap)",
  render: () => <ToggleDemo />,
};
