import { useEffect, useState } from "react";
import Button from "../../atoms/Button";
import Slider from "../../atoms/Slider";
import { useAudioMixer, type AudioSource } from "../../overlay/audioMixer";
import "./audiomixer.css";

const fileLabel = (value: string) => value.replace(/^\.+/, "").replace(/\.[a-z0-9]+$/i, "").replace(/([a-z])([A-Z0-9])/g, "$1 $2").replace(/[_-]+/g, " ").replace(/^./, c => c.toUpperCase());
const sourceLabel = (source: AudioSource) => {
  const entity = source.name.match(/^(.*) \u00b7 SceneEntityId \{ id: (\d+), generation: \d+ \}$/);
  return entity ? `${fileLabel(entity[1]!)} \u00b7 #${entity[2]}` : source.name === source.source ? fileLabel(source.name) : source.name;
};

const typeLabel = (type: string) => ({ "Sound effects": "Scene volume", Video: "Video stream", "Avatar sounds": "Avatar and emote sounds", Interface: "Menus and buttons", Voice: "Voice chat" }[type] ?? type);
const distanceLabel = (source: AudioSource) => typeof source.distance === "number" && Number.isFinite(source.distance) && source.distance >= 0 ? ` \u00b7 ${Math.round(source.distance)} m away` : "";

export default function AudioMixer({ advanced = false }: { advanced?: boolean }) {
  const mixer = useAudioMixer();
  const [groupBy, setGroupBy] = useState("area");
  const [search, setSearch] = useState("");
  useEffect(() => mixer?.watch(), [mixer?.watch]);
  if (!mixer) return null;
  const live = new Map(mixer.sources.map(source => [source.id, source]));
  const pinned = Object.values(mixer.saved).map(source => ({ ...source, ...live.get(source.id), volume: source.id.startsWith("voice:") ? (live.get(source.id)?.volume ?? source.volume) : source.volume }));
  const controls = (source: AudioSource) => <li key={source.id} className="audio-mixer__source">
    <div className="audio-mixer__source-heading"><div><strong>{sourceLabel(source)}</strong><small>{source.area} &middot; {typeLabel(source.type)}{distanceLabel(source)}</small></div><button type="button" aria-label={`Remove ${sourceLabel(source)} control`} title="Remove control and restore source volume" disabled={!!mixer.busy} onClick={() => void mixer.remove(source)}>Remove</button></div>
    <small>{!live.has(source.id) ? "Currently unavailable \u00b7 preference saved" : !source.inScene ? "Outside your current scene \u00b7 silent" : source.playing ? "Playing" : "Ready"}</small>
    <div className="audio-mixer__volume"><Slider ariaLabel={`${sourceLabel(source)} volume`} min={0} max={source.id.startsWith("voice:") ? 200 : 100} value={source.volume * 100} disabled={!!mixer.busy || (!mixer.connected && !source.id.startsWith("voice:"))} format={v => `${Math.round(v)}%`} onCommit={v => void mixer.setVolume(source, v / 100)} /><button type="button" aria-label={`${source.volume === 0 ? "Unmute" : "Mute"} ${sourceLabel(source)}`} aria-pressed={source.volume === 0} disabled={!!mixer.busy} onClick={() => void mixer.setVolume(source, source.volume === 0 ? (mixer.saved[source.id]?.previousVolume || 1) : 0)}>{source.volume === 0 ? "Unmute" : "Mute"}</button></div>
  </li>;
  const available = mixer.sources.filter(source => !mixer.saved[source.id] && `${source.area} ${source.type} ${source.source ?? ""} ${sourceLabel(source)}`.toLowerCase().includes(search.toLowerCase()));
  const groups = new Map<string, AudioSource[]>();
  for (const source of available) {
    const group = groupBy === "type" ? typeLabel(source.type) : groupBy === "source" ? fileLabel(source.source || source.name) : source.area;
    groups.set(group, [...(groups.get(group) ?? []), source]);
  }
  return <section className="audio-mixer" aria-label="Individual sound controls">
    {mixer.error && <p role="alert">{mixer.error}</p>}
    {pinned.length > 0 && <><h3>Your sound controls</h3><ul className="audio-mixer__sources">{pinned.map(controls)}</ul></>}
    {advanced && <>
      <p>Add a source to control its volume independently. Scene sources are silent outside their scene. Removing a control restores its original volume.</p>
      <div className="audio-mixer__filters"><input type="search" aria-label="Find sound source" placeholder="Find a sound or scene" value={search} onChange={e => setSearch(e.target.value)} /><label>Group by<select value={groupBy} onChange={e => setGroupBy(e.target.value)}><option value="area">Area</option><option value="type">Type</option><option value="source">Source</option></select></label></div>
      {!mixer.connected && <p role="status">Waiting for the engine&rsquo;s audio sources&hellip;</p>}
      {mixer.connected && !available.length && <p role="status">{search ? "No sources match your search." : mixer.sources.length ? "All available sources are in your controls." : "No sound sources in this area yet."}</p>}
      {[...groups.entries()].sort(([a], [b]) => a.localeCompare(b)).map(([group, sources]) => <section className="audio-mixer__group" key={group}><h3>{group}</h3><ul>{sources.map(source => <li key={source.id}><div><strong>{sourceLabel(source)}</strong><small>{groupBy === "area" ? typeLabel(source.type) : source.area}{distanceLabel(source)}{!source.inScene ? " \u00b7 Outside current scene" : ""}</small></div><Button variant="secondary" size="sm" data-audio-source={source.id} aria-label={`Add ${sourceLabel(source)} control`} onClick={() => mixer.pin(source)}>Add</Button></li>)}</ul></section>)}
    </>}
  </section>;
}
