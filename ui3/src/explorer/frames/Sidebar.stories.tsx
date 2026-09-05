import type { CSSProperties } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import { useState } from "react";
import Sidebar from "./Sidebar";
import Chat from "./ChatBridge";
import EmoteWheel from "../components/EmoteWheel";
import Notifications from "../components/Notifications";
import VoiceChat from "../components/VoiceChat";
import SkyboxHUD from "../components/SkyboxHUD";
import ProfileWidget from "../components/ProfileWidget";
import "../../overlay/overlay.css";

const meta = {
  title: "Explorer/Frames/Sidebar",
  component: Sidebar,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof Sidebar>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => <Sidebar />,
};

const STAGE: CSSProperties = {
  position: "fixed",
  inset: 0,
  background:
    "radial-gradient(120% 90% at 70% 10%, #3a2a52 0%, #1d1530 55%, #120c1f 100%)",
  overflow: "hidden",
};

function SidebarReview() {
  const [profileOpen, setProfileOpen] = useState(false);
  const [chatOpen, setChatOpen] = useState(false);
  const [emoteOpen, setEmoteOpen] = useState(false);
  const [notifOpen, setNotifOpen] = useState(false);
  const [voiceOpen, setVoiceOpen] = useState(false);
  const [skyboxOpen, setSkyboxOpen] = useState(false);

  return (
    <div style={STAGE}>
      <div className="ui3-overlay" data-live="true">
        <div className="ui3-overlay__widget ui3-overlay__sidebar">
          <Sidebar
            onProfileToggle={() => setProfileOpen((o) => !o)}
            chatOpen={chatOpen}
            onChatToggle={() => setChatOpen((o) => !o)}
            emoteOpen={emoteOpen}
            onEmoteToggle={() => setEmoteOpen((o) => !o)}
            notifOpen={notifOpen}
            onNotifToggle={() => setNotifOpen((o) => !o)}
            voiceOpen={voiceOpen}
            onVoiceToggle={() => setVoiceOpen((o) => !o)}
            skyboxOpen={skyboxOpen}
            onSkyboxToggle={() => setSkyboxOpen((o) => !o)}
          />
        </div>

        <div className="ui3-overlay__widget ui3-overlay__profile">
          <ProfileWidget
            open={profileOpen}
            name="CosmicLux"
            tag="#92e6"
            wallet={"0x629\u{2026}92e6"}
            address="0x629000000000000000000000000000000000092e6"
            isGuest={false}
          />
        </div>

        <div className="ui3-overlay__widget ui3-overlay__chat">
          <Chat open={chatOpen} onToggle={() => setChatOpen((o) => !o)} />
        </div>
        {emoteOpen && (
          <div className="ui3-overlay__widget ui3-overlay__emote">
            <EmoteWheel
              onSelect={() => setEmoteOpen(false)}
              onClose={() => setEmoteOpen(false)}
            />
          </div>
        )}
        {notifOpen && (
          <div className="ui3-overlay__widget ui3-overlay__notifications">
            <Notifications bare />
          </div>
        )}
        {voiceOpen && (
          <div className="ui3-overlay__widget ui3-overlay__voice">
            <VoiceChat bare />
          </div>
        )}
        {skyboxOpen && (
          <div className="ui3-overlay__widget ui3-overlay__skybox">
            <SkyboxHUD />
          </div>
        )}
      </div>
    </div>
  );
}

export const Review: Story = {
  render: () => <SidebarReview />,
};
