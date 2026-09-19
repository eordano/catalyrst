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
          "nothing: Enter or the sidebar Chat button opens it with the input focused. " +
          "Nearby, Messages and Communities share the ui3 message layout and composer. " +
          "Signed-in users can search friends and communities, open a conversation, " +
          "and retain their drafts across channel switches. New messages preserve " +
          "the reader's scroll position and offer a jump to the latest message.",
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
