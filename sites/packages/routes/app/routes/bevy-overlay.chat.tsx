import { useCallback, useState } from "react";
import { useSearchParams } from "react-router";

import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import ClientStage from "@ui/overlay/panels/ClientStage";
import Chat from "@ui/explorer/frames/ChatBridge";

import type { Route } from "./+types/bevy-overlay.chat";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "client/chat-open";

function isPanelOpen(raw: string | null): boolean {
  return raw === null || raw === "chat";
}

const FALLBACK: Assignment = {
  variant: "sidebar-chat",
  flags: { urlAddressable: true },
  experimentKey: "client_chat_open",
};

export async function loader({ request }: Route.LoaderArgs) {
  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const payload = { sid, assignment };

  return wrap(payload);
}

export default function ChatRoute({ loaderData }: Route.ComponentProps) {
  return <ChatStage assignment={loaderData.assignment} />;
}

type StageProps = {
  assignment: Assignment;
};

function ChatStage({ assignment }: StageProps) {
  const [searchParams, setSearchParams] = useSearchParams();

  const urlAddressable = assignment.flags.urlAddressable === true;

  const [localOpen, setLocalOpen] = useState(true);
  const open = urlAddressable
    ? isPanelOpen(searchParams.get("panel"))
    : localOpen;

  const onToggle = useCallback(() => {
    if (!urlAddressable) {
      setLocalOpen((o) => !o);
      return;
    }
    setSearchParams(
      (prev) => {
        const next = new URLSearchParams(prev);
        if (open) next.set("panel", "closed");
        else next.set("panel", "chat");
        return next;
      },
      { preventScrollReset: true },
    );
  }, [urlAddressable, open, setSearchParams]);

  return (
    <ClientStage nojs="Enable JavaScript to chat with people nearby.">
      <Chat open={open} onToggle={onToggle} />
    </ClientStage>
  );
}
