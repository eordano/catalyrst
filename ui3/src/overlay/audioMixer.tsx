import { createContext, useCallback, useContext, useEffect, useRef, useState, type ReactNode } from "react";
import { useBridgeState } from "./bridge";
import { useVoiceParticipants } from "./voiceParticipants";

export type AudioSource = { id: string; name: string; source?: string; area: string; type: string; playing: boolean; inScene: boolean; volume: number; distance?: number | null };
type SavedSource = AudioSource & { previousVolume?: number };
const STORAGE = "dcl.audio.sources.v1";
function isSource(raw: unknown): raw is SavedSource {
  if (!raw || typeof raw !== "object") return false;
  const s = raw as SavedSource;
  const max = typeof s.id === "string" && s.id.startsWith("voice:") ? 2 : 1;
  return /^(?:[a-f0-9]{16}|voice:0x[a-f0-9]{40})$/.test(s.id)
    && typeof s.name === "string" && typeof s.area === "string" && typeof s.type === "string"
    && (s.source === undefined || typeof s.source === "string")
    && typeof s.playing === "boolean" && typeof s.inScene === "boolean"
    && Number.isFinite(s.volume) && s.volume >= 0 && s.volume <= max
    && (s.previousVolume === undefined || (Number.isFinite(s.previousVolume) && s.previousVolume > 0 && s.previousVolume <= max));
}
export function readAudioSources(): Record<string, SavedSource> {
  try {
    const saved = JSON.parse(localStorage.getItem(STORAGE) || "{}");
    return Object.fromEntries(Object.entries(saved).filter(([id, raw]) => isSource(raw) && raw.id === id)) as Record<string, SavedSource>;
  } catch { return {}; }
}
async function engineCommand(command: string): Promise<{ sources: AudioSource[] }> {
  const send = window.engine_console_command;
  if (!send) throw new Error("Engine unavailable");
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    const response = await Promise.race([
      send(command),
      new Promise<never>((_, reject) => { timer = setTimeout(() => reject(new Error("Audio request timed out")), 8000); }),
    ]);
    const result = JSON.parse(response) as { sources?: unknown };
    if (!Array.isArray(result.sources) || !result.sources.every(isSource)) throw new Error("Invalid audio sources");
    return { sources: result.sources };
  } finally { clearTimeout(timer); }
}

type Mixer = {
  sources: AudioSource[]; saved: Record<string, SavedSource>; error: string; connected: boolean; busy: string | null;
  watch: () => () => void;
  pin: (source: AudioSource) => void;
  remove: (source: AudioSource) => Promise<void>;
  setVolume: (source: AudioSource, volume: number) => Promise<void>;
};
const AudioMixerContext = createContext<Mixer | null>(null);
export const useAudioMixer = () => useContext(AudioMixerContext);

export function AudioMixerProvider({ children }: { children: ReactNode }) {
  const [sources, setSources] = useState<AudioSource[]>([]);
  const [saved, setSaved] = useState(readAudioSources);
  const preferences = useRef(saved);
  const [error, setError] = useState("");
  const [connected, setConnected] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const viewers = useRef(0);
  const changing = useRef(false);
  const blocked = useBridgeState(s => s.friends.blocked);
  const { participants, setVolume: setVoiceVolume } = useVoiceParticipants();
  const watch = useCallback(() => { viewers.current++; return () => { viewers.current--; }; }, []);
  const persist = (next: Record<string, SavedSource>) => {
    preferences.current = next;
    setSaved(next);
    try { localStorage.setItem(STORAGE, JSON.stringify(next)); }
    catch { setError("Volume changed for this visit. Allow browser storage to remember it."); }
  };
  useEffect(() => {
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout>;
    let restored: typeof window.engine_console_command;
    const poll = async () => {
      try {
        const command = window.engine_console_command;
        if (!command) { if (!cancelled) setConnected(false); return; }
        if (restored !== command) {
          for (const source of Object.values(preferences.current)) {
            if (cancelled) return;
            if (source.id.startsWith("voice:")) setVoiceVolume(source.id.slice(6), source.volume);
            else await engineCommand(`/audio_sources --set ${source.id} --volume ${source.volume}`);
          }
          restored = command;
        }
        if (!viewers.current) return;
        const result = await engineCommand("/audio_sources");
        if (!cancelled) { setSources(result.sources); setConnected(true); setError(""); }
      } catch {
        if (!cancelled) { setConnected(false); setError("Could not refresh audio sources. Retrying\u2026"); }
      } finally { if (!cancelled) timer = setTimeout(() => void poll(), 1500); }
    };
    void poll();
    return () => { cancelled = true; clearTimeout(timer); };
  }, [setVoiceVolume]);
  const change = async (source: AudioSource, volume: number, remove = false) => {
    if (changing.current || !Number.isFinite(volume) || volume < 0 || volume > (source.id.startsWith("voice:") ? 2 : 1)) return;
    changing.current = true;
    setBusy(source.id); setError("");
    try {
      if (source.id.startsWith("voice:")) setVoiceVolume(source.id.slice(6), volume);
      else {
        if (!window.engine_console_command) throw new Error();
        await engineCommand(`/audio_sources --set ${source.id} --volume ${volume}`);
      }
      const next = { ...preferences.current };
      if (remove) delete next[source.id];
      else next[source.id] = { ...source, volume, previousVolume: volume === 0 ? (source.volume || next[source.id]?.previousVolume || 1) : volume };
      persist(next);
      setSources(previous => previous.map(s => s.id === source.id ? { ...s, volume } : s));
    } catch { setError("Could not change this source. Try again."); }
    finally { changing.current = false; setBusy(null); }
  };
  const voice: AudioSource[] = participants.filter(p => !blocked.some(address => address.toLowerCase() === p.address.toLowerCase())).map(p => ({ id: `voice:${p.address.toLowerCase()}`, name: p.name, source: p.name, area: "Nearby people", type: "Voice chat", playing: p.speaking, inScene: true, volume: p.volume }));
  return <AudioMixerContext value={{ sources: [...sources, ...voice], saved, error, connected, busy, watch,
    pin: source => persist({ ...preferences.current, [source.id]: { ...source } }),
    remove: source => change(source, 1, true), setVolume: (source, volume) => change(source, volume),
  }}>{children}</AudioMixerContext>;
}
