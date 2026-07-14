import fs from "node:fs";
import path from "node:path";

import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";

import VoiceJoin, { type VoiceSession } from "@features/stories/overlay/voice-join/VoiceJoin";

import type { Route } from "./+types/bevy-overlay.voice-join";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "overlay/voice-join";
const FIXTURE = path.join(process.cwd(), "packages", "data", "src", "fixtures", `${STORY}.json`);

const FALLBACK_SESSION: VoiceSession = {
  kind: "private",
  roomId: "call-9f3a21",
  roomName: "voice-chat-private-call-9f3a21",
  peerName: "Nyx",
  selfAddress: "0x0f5d2fb29fb7d3cfee444a200298f468908cc942",
  peerAddress: "0x6b7d9e3a1c2f4b5a8d0e1f2c3b4a5968d7e6f5a4",
  pushToTalkKey: "T",
};

function loadSession(): VoiceSession {
  try {
    const raw = JSON.parse(fs.readFileSync(FIXTURE, "utf8")) as {
      session?: Partial<VoiceSession> & { initialMicEnabled?: boolean };
    };
    const s = raw.session;
    if (!s) return FALLBACK_SESSION;
    return {
      kind: s.kind === "community" ? "community" : "private",
      roomId: s.roomId ?? FALLBACK_SESSION.roomId,
      roomName: s.roomName ?? FALLBACK_SESSION.roomName,
      peerName: s.peerName ?? FALLBACK_SESSION.peerName,
      selfAddress: s.selfAddress ?? FALLBACK_SESSION.selfAddress,
      peerAddress: s.peerAddress ?? FALLBACK_SESSION.peerAddress,
      pushToTalkKey: s.pushToTalkKey ?? FALLBACK_SESSION.pushToTalkKey,
    };
  } catch {
    return FALLBACK_SESSION;
  }
}

const FALLBACK: Assignment = {
  variant: "wizard",
  flags: { wizard: true, pushToTalk: true },
  experimentKey: "cl_voice_join",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const step = url.searchParams.get("step")?.trim() || null;

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  const payload = { sid, step, assignment, session: loadSession() };
  return wrap(payload);
}

export default function BevyOverlayVoiceJoin({ loaderData }: Route.ComponentProps) {
  const { sid, step, assignment, session } = loaderData;

  return (
    <main className="bevy-overlay-voice-join">
      <VoiceJoin
        trackCtx={{
          sid,
          story: STORY,
          variant: assignment.variant,
          experimentKey: assignment.experimentKey,
        }}
        session={session}
        initialStep={step ?? undefined}
      />
    </main>
  );
}
