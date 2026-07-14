import { useEffect, useRef } from "react";
import { useMachine } from "@xstate/react";
import { useSearchParams } from "react-router";

import VoiceJoinView from "@ui/overlay/workflows/VoiceJoinView";

import type { TrackContext } from "@core/lib/telemetry/track";
import {
  voiceMachine,
  resolveVoiceSnapshot,
  slugToState,
  slugIsMuted,
  stateToSlug,
  type ConnectFn,
  type TrackFn,
  type VoiceKind,
} from "./machine";

export type VoiceSession = {
  kind: VoiceKind;
  roomId: string;
  roomName: string;
  peerName: string;
  selfAddress: string;
  peerAddress: string;
  pushToTalkKey: string;
};

export type VoiceJoinProps = {
  trackCtx: TrackContext;
  session: VoiceSession;
  initialStep?: string;
  initialMuted?: boolean;
  connect?: ConnectFn;
  track?: TrackFn;
};

export default function VoiceJoin(props: VoiceJoinProps) {
  const [searchParams] = useSearchParams();

  const rawStep = (searchParams.get("step")?.trim() || props.initialStep) ?? undefined;
  const stateId = slugToState(rawStep);
  const muted = slugIsMuted(rawStep) || !!props.initialMuted;

  return (
    <VoiceJoinInner
      key={`${stateId}:${muted ? "m" : "u"}`}
      stateId={stateId}
      muted={muted}
      {...props}
    />
  );
}

type InnerProps = VoiceJoinProps & {
  stateId: ReturnType<typeof slugToState>;
  muted: boolean;
};

function VoiceJoinInner({
  stateId,
  muted,
  trackCtx,
  session,
  connect,
  track,
}: InnerProps) {
  const [, setSearchParams] = useSearchParams();

  const snapshot = useRef(
    resolveVoiceSnapshot({
      step: stateId,
      trackCtx,
      connect,
      track,
      roomId: session.roomId,
      kind: session.kind,
      muted,
    }),
  ).current;

  const [state, send] = useMachine(voiceMachine, {
    input: { trackCtx, connect, track, roomId: session.roomId },
    snapshot,
  });

  const value = state.value as string;
  const step = stateToSlug(value);
  const ctx = state.context;

  const urlStep = value === "talking" && ctx.micMuted ? "mute" : step;

  const lastStep = useRef<string | null>(null);
  useEffect(() => {
    if (lastStep.current === urlStep) return;
    lastStep.current = urlStep;
    setSearchParams(
      (prev) => {
        const params = new URLSearchParams(prev);
        params.set("panel", "voice");
        if (params.get("step") === urlStep) return params;
        params.set("step", urlStep);
        return params;
      },
      { replace: true, preventScrollReset: true },
    );
  }, [urlStep, setSearchParams]);

  return (
    <VoiceJoinView
      value={value}
      step={urlStep}
      session={session}
      kind={ctx.kind}
      micMuted={ctx.micMuted}
      resultRoomName={ctx.result?.roomName}
      error={ctx.error}
      onRequest={(kind) => send({ type: "REQUEST", kind })}
      onToggleMute={() => send({ type: "TOGGLE_MUTE" })}
      onLeave={() => send({ type: "LEAVE" })}
      onRetry={() => send({ type: "RETRY" })}
    />
  );
}
