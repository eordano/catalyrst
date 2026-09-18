import type { Meta, StoryObj } from "@storybook/react-vite";
import type { ReactNode } from "react";
import FloatingPanel, { type FloatingPanelId } from "./FloatingPanel";
import { VoiceControls } from "./VoiceChat";
import { SkyboxControls } from "./SkyboxHUD";
import Notifications from "./Notifications";
import Friends from "../pages/Friends";
import SmartWearablesPanel from "../../app/panels/SmartWearables.route";
import Sidebar from "../frames/Sidebar";
import { MinimapVisibilityProvider } from "../../overlay/minimapVisibility";
import "../../overlay/overlay.css";

const WORLD =
  "radial-gradient(120% 90% at 30% 10%, #3a2f5c 0%, #1c1830 45%, #0c0a14 100%)";

const noop = () => {};

export function HudPanelScene({ panel, children }: { panel: FloatingPanelId; children: ReactNode }) {
  return (
    <div style={{ position: "fixed", inset: 0, background: WORLD }}>
      <MinimapVisibilityProvider>
        <div className="ui3-overlay" data-live="true">
          <div className="ui3-overlay__widget ui3-overlay__sidebar">
            <Sidebar
              notifOpen={panel === "notifications"}
              voiceOpen={panel === "voice"}
              skyboxOpen={panel === "skybox"}
              portablesOpen={panel === "portables"}
              friendsOpen={panel === "friends"}
            />
          </div>
          <div className={"ui3-overlay__widget ui3-overlay__" + panel}>{children}</div>
        </div>
      </MinimapVisibilityProvider>
    </div>
  );
}

const FRIENDS = [
  { name: "Nyx", tag: "#a91f", online: true, status: "online" as const, where: "Genesis Plaza", hue: 280, address: "0x1111111111111111111111111111111111aaaa", hasClaimedName: true },
  { name: "pixelwitch", tag: "#0c2d", online: true, status: "away" as const, where: "Soul Magic", hue: 320, address: "0x2222222222222222222222222222222222bbbb" },
  { name: "Maple", tag: "#33ab", online: false, where: "Offline", hue: 95, address: "0x4444444444444444444444444444444444dddd" },
];

const meta = {
  title: "Explorer/Components/FloatingPanel",
  component: FloatingPanel,
  parameters: { layout: "fullscreen" },
  args: { id: "voice", onClose: noop, children: null },
  excludeStories: ["HudPanelScene"],
} satisfies Meta<typeof FloatingPanel>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Surface: Story = {
  parameters: { layout: "centered", sceneBackdrop: false },
  render: () => (
    <div style={{ position: "relative", width: 460, height: 260 }}>
      <FloatingPanel id="voice" title="Panel title" onClose={noop}>
        <p style={{ margin: 0 }}>Every floating HUD panel shares this surface, header and close button.</p>
      </FloatingPanel>
    </div>
  ),
};

export const Voice: Story = {
  render: () => (
    <HudPanelScene panel="voice">
      <FloatingPanel id="voice" onClose={noop}>
        <VoiceControls />
      </FloatingPanel>
    </HudPanelScene>
  ),
};

export const Portables: Story = {
  render: () => (
    <HudPanelScene panel="portables">
      <FloatingPanel id="portables" onClose={noop}>
        <SmartWearablesPanel floating />
      </FloatingPanel>
    </HudPanelScene>
  ),
};

export const Skybox: Story = {
  render: () => (
    <HudPanelScene panel="skybox">
      <FloatingPanel id="skybox" onClose={noop}>
        <SkyboxControls />
      </FloatingPanel>
    </HudPanelScene>
  ),
};

export const FriendsList: Story = {
  name: "Friends",
  render: () => (
    <HudPanelScene panel="friends">
      <FloatingPanel id="friends" onClose={noop} flush>
        <Friends floating friends={FRIENDS} />
      </FloatingPanel>
    </HudPanelScene>
  ),
};

export const NotificationsList: Story = {
  name: "Notifications",
  render: () => (
    <HudPanelScene panel="notifications">
      <FloatingPanel id="notifications" onClose={noop} flush>
        <Notifications bare floating />
      </FloatingPanel>
    </HudPanelScene>
  ),
};
