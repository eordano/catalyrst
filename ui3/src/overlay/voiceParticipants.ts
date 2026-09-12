import { useCallback, useEffect, useState } from "react";

import type { VoiceParticipant } from "../generated/bridge/VoiceParticipant";
import { attachBridge, sendBridge } from "./bridge";

function isVoiceParticipantsPush(
  push: unknown,
): push is { participants: VoiceParticipant[] } {
  const p = push as { kind?: unknown; participants?: unknown } | null;
  return !!p && p.kind === "voiceParticipants" && Array.isArray(p.participants);
}

export function useVoiceParticipants(): {
  participants: VoiceParticipant[];
  setVolume: (address: string, volume: number) => void;
} {
  const [participants, setParticipants] = useState<VoiceParticipant[]>([]);

  useEffect(
    () =>
      attachBridge((push) => {
        if (!isVoiceParticipantsPush(push)) return;
        setParticipants(push.participants);
      }),
    [],
  );

  const setVolume = useCallback((address: string, volume: number) => {
    setParticipants((prev) =>
      prev.map((p) => (p.address === address ? { ...p, volume } : p)),
    );
    sendBridge("SetVoiceParticipantVolume", { address, volume });
  }, []);

  return { participants, setVolume };
}
