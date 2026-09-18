import type { Meta, StoryObj } from "@storybook/react-vite";
import { useState } from "react";
import Chat from "./ChatBridge";

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
          "`useBridgeState().chat` / `.players` and submits via `sendBridge(\"SendChat\", \u{2026})`, " +
          "and it is the chat that `app/AppLayout.tsx` mounts at runtime. Closed, it renders " +
          "nothing: Enter or the sidebar Chat button opens it with the input focused. Two " +
          "visible states: open + idle (translucent \u{2014} bubbles float over the world), and " +
          "open + active on hover/focus (solid panel with navbar, emoji picker, and the " +
          "nearby-members list). The log auto-scrolls to the newest message.",
      },
    },
  },
  args: {
    open: true,
    onToggle: () => {},
  },
} satisfies Meta<typeof Chat>;

export default meta;
type Story = StoryObj<typeof meta>;

function Opened() {
  const [open, setOpen] = useState(true);
  return <Chat open={open} onToggle={() => setOpen((o) => !o)} />;
}

export const Default: Story = {
  render: () => <Opened />,
};
