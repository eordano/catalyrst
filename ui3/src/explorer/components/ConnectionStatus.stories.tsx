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

export const Healthy: Story = {
  render: () => (
    <Frame>
      <ConnectionStatus
        connection={{ sceneHealth: "ok", sceneRoom: false, globalRoom: true }}
        realm="catalyst.example.com"
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
      />
    </Frame>
  ),
};

export const Connecting: Story = {
  render: () => (
    <Frame>
      <ConnectionStatus connection={null} realm={null} />
    </Frame>
  ),
};
