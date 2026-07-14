import { useState } from "react";
import Chat from "../../explorer/frames/ChatBridge";

export default function ChatPanel() {
  const [open, setOpen] = useState(true);
  return <Chat open={open} onToggle={() => setOpen((o) => !o)} />;
}
