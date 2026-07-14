import { useMemo } from "react";

import Toggle from "../../atoms/Toggle";
import { useBridgeState } from "../../overlay/bridge";
import { useVoiceParticipants } from "../../overlay/voiceParticipants";

export default function VoiceParticipantList() {
  const { participants: roster, setVolume } = useVoiceParticipants();
  const blocked = useBridgeState((s) => s.friends.blocked);

  // Backstop behind the engine-side filter in push_voice_participants: a roster push
  // already replayed by the loader when the block landed must not name the blocked
  // user either.
  const participants = useMemo(() => {
    const blockedSet = new Set(blocked.map((a) => a.toLowerCase()));
    return roster.filter((p) => !blockedSet.has(p.address.toLowerCase()));
  }, [roster, blocked]);

  if (participants.length === 0) {
    return (
      <div className="set__voice-empty">No one is in voice chat right now.</div>
    );
  }

  return (
    <ul className="set__voice">
      {participants.map((p) => {
        const muted = p.volume === 0;
        return (
          <li className="set__voice-row" key={p.address}>
            <span
              className={"set__voice-dot" + (p.speaking ? " is-speaking" : "")}
              aria-hidden="true"
            />
            <span className="set__voice-name">{p.name}</span>
            {p.speaking && <span className="set__vh">speaking</span>}
            <span className="set__voice-addr">
              {p.address.slice(0, 6)}&#x2026;{p.address.slice(-4)}
            </span>
            <span className="set__voice-mute">
              <span className="set__voice-mutelabel">Mute</span>
              <Toggle
                ariaLabel={"Mute " + p.name}
                checked={muted}
                onChange={(next: boolean) => setVolume(p.address, next ? 0 : 1)}
              />
            </span>
          </li>
        );
      })}
    </ul>
  );
}
