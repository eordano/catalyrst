import { useState } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import SidebarDesign, { type SidebarDrawer } from "./SidebarDesign";
import { MinimapVisibilityProvider } from "../../overlay/minimapVisibility";
import "../../overlay/overlay.css";
import "./sidebar-popovers.css";

const meta = { title: "Explorer/Frames/Sidebar September 2026", component: SidebarDesign, parameters: { layout: "fullscreen" } } satisfies Meta<typeof SidebarDesign>;
export default meta;
type Story = StoryObj<typeof meta>;

function Preview({ initial }: { initial: SidebarDrawer | null }) {
  const [drawer, setDrawer] = useState(initial);
  const [action, setAction] = useState("");
  return <MinimapVisibilityProvider><div style={{ position: "fixed", inset: 0, background: "#6d7c83" }}>
    <p style={{ position: "absolute", right: 24, bottom: 24 }}>Staged HUD preview{action && `: ${action}`}</p>
    <div className="ui3-overlay" data-sidebar-design="2026-09-sidebar-design" onClick={(event) => {
      const link = (event.target as Element).closest("[data-sb-linkto]")?.getAttribute("data-sb-linkto");
      if (link) setAction(link);
    }}><SidebarDesign drawer={drawer} onDrawerChange={setDrawer} onConnectionToggle={() => setAction("Connection & performance")}
      onProfileToggle={() => setAction("Profile")} onChatToggle={() => setAction("Chat")} onVoiceToggle={() => setAction("Voice")}
      onFriendsToggle={() => setAction("Friends")} onEmoteToggle={() => setAction("Emotes")}
      onSkyboxToggle={() => setAction("Time of day")} onPortablesToggle={() => setAction("Portable experiences")} onNotifToggle={() => setAction("Notifications")} />
    </div>
  </div></MinimapVisibilityProvider>;
}

export const Dock: Story = { args: { drawer: null, onDrawerChange: () => {}, onConnectionToggle: () => {} }, render: () => <Preview initial={null} /> };
export const Me: Story = { ...Dock, render: () => <Preview initial="me" /> };
export const Controls: Story = { ...Dock, render: () => <Preview initial="system" /> };

export const Discover: Story = { ...Dock, render: () => <Preview initial="discover" /> };
