import type { Meta, StoryObj } from "@storybook/react-vite";
import { useState } from "react";
import Chat from "./Chat";

const meta = {
  title: "Explorer/Frames/Chat",
  component: Chat,
  parameters: {
    layout: "centered",
    sceneBackdrop: false,
    docs: {
      description: {
        component:
          "**Chat is the canonical, live in-world chat.** This is the FUNCTIONAL " +
          "Nearby-chat island actually wired into the HUD: it reads " +
          "`useBridgeState().chat` / `.players` and submits via `sendBridge(\"SendChat\", …)`, " +
          "and it is the chat that `app/AppLayout.tsx` mounts at runtime. Three states: " +
          "collapsed (borderless floating input bar), open + idle (translucent — bubbles " +
          "float over the world), and open + active on hover/focus (solid panel with " +
          "navbar, emoji picker, and the nearby-members list). The log auto-scrolls to " +
          "the newest message.",
      },
    },
  },
  args: {
    open: false,
    onToggle: () => {},
  },
} satisfies Meta<typeof Chat>;

export default meta;
type Story = StoryObj<typeof meta>;

function Collapsed() {
  const [open, setOpen] = useState(false);
  return <Chat open={open} onToggle={() => setOpen((o) => !o)} />;
}

function Opened() {
  const [open, setOpen] = useState(true);
  return <Chat open={open} onToggle={() => setOpen((o) => !o)} />;
}

export const Default: Story = {
  render: () => <Collapsed />,
};

export const Open: Story = {
  render: () => <Opened />,
};
