import type { ReactNode } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import ConnectionStatus from "./ConnectionStatus";

const meta = {
  title: "Explorer/Components/ConnectionStatus",
  component: ConnectionStatus,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof ConnectionStatus>;

export default meta;
type Story = StoryObj<typeof meta>;

const Frame = ({ children }: { children: ReactNode }) => (
  <div
    style={{
      minHeight: "100vh",
      display: "flex",
      alignItems: "center",
      justifyContent: "center",
      background: "#0a0a0f",
    }}
  >
    {children}
  </div>
);

const PINNED = {
  at: new Date("2026-09-17T17:01:29.000Z"),
  userAgent: "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 Chrome/153.0.0.0 Safari/537.36",
  overlayBuild: "AppShell-CSUtUagR",
};

const ABOUT = {
  realmName: "dcl-one",
  commsProtocol: "v3",
  commsAdapter: "archipelago:archipelago:wss://catalyst.example.com/ws",
  commsVersion: "24.18.0",
  contentVersion: "8.0.3",
  lambdasVersion: "4.12.0",
  usersCount: 12,
};

const SCENE_ID = "bafkreibg66k5ipkfaego4qccbesmtfcpa6xmpzkmghm27qbl7pxbq7g5wm";

export const Healthy: Story = {
  render: () => (
    <Frame>
      <ConnectionStatus
        connection={{ sceneHealth: "ok", sceneRoom: false, globalRoom: true }}
        realm="catalyst.example.com"
        fps={{ page: 60, engine: 60, ms: 16.7 }}
        about={null}
        sceneId={null}
        {...PINNED}
      />
    </Frame>
  ),
};

export const Degraded: Story = {
  render: () => (
    <Frame>
      <ConnectionStatus
        connection={{ sceneHealth: "error", sceneRoom: true, globalRoom: false }}
        realm="catalyst.example.com"
        fps={{ page: 34, engine: 59, ms: 29.4 }}
        about={null}
        sceneId={null}
        {...PINNED}
      />
    </Frame>
  ),
};

export const Connecting: Story = {
  render: () => (
    <Frame>
      <ConnectionStatus
        connection={null}
        realm={null}
        fps={{ page: 60, engine: null, ms: 16.7 }}
        about={null}
        sceneId={null}
        {...PINNED}
      />
    </Frame>
  ),
};

export const WithDebugInfo: Story = {
  render: () => (
    <Frame>
      <ConnectionStatus
        connection={{ sceneHealth: "ok", sceneRoom: true, globalRoom: true }}
        realm="dcl-one"
        fps={{ page: 58, engine: 61, ms: 17.2 }}
        sceneId={SCENE_ID}
        about={ABOUT}
        {...PINNED}
      />
    </Frame>
  ),
};

export const DegradedFrameRate: Story = {
  render: () => (
    <Frame>
      <ConnectionStatus
        connection={{ sceneHealth: "error", sceneRoom: false, globalRoom: true }}
        realm="dcl-one"
        fps={{ page: 21, engine: 19, ms: 47.6 }}
        sceneId={null}
        about={null}
        {...PINNED}
      />
    </Frame>
  ),
};
