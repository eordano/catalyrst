import { useId, useState } from "react";
import { newSpawnArea, type SpawnAreaDraft } from "../spawn-areas";

export default function DeSpawnAreas({ areas, onChange }: { areas: SpawnAreaDraft[]; onChange(areas: SpawnAreaDraft[]): void }) {
  const id = useId();
  const [selected, setSelected] = useState(() => Math.max(0, areas.findIndex(area => area.default)));
  const index = Math.min(selected, areas.length - 1);
  const area = areas[index];
  const change = (key: keyof Omit<SpawnAreaDraft, "source" | "default">, value: string) => onChange(areas.map((area, i) => i === index ? { ...area, [key]: value } : area));
  const field = (key: keyof Omit<SpawnAreaDraft, "source" | "default">, label: string) => <div className="descene-field">
    <label htmlFor={`${id}-${key}`}>{label}</label>
    <input id={`${id}-${key}`} value={area![key]} onChange={event => change(key, event.target.value)} />
  </div>;
  const add = (duplicate?: SpawnAreaDraft) => {
    onChange([...areas, newSpawnArea(areas, duplicate)]);
    setSelected(areas.length);
  };
  const remove = () => {
    const remaining = areas.filter((_, i) => i !== index);
    if (area?.default && remaining[0]) remaining[0] = { ...remaining[0], default: true };
    onChange(remaining);
    setSelected(Math.max(0, index - 1));
  };
  return <section aria-label="Spawn areas" className="descene-spawns">
    <div className="descene-field">
      <label htmlFor={`${id}-area`}>Spawn area</label>
      <select id={`${id}-area`} value={index} disabled={!areas.length} onChange={event => setSelected(Number(event.target.value))}>
        {!areas.length && <option value={-1}>No spawn areas</option>}
        {areas.map((area, i) => <option key={i} value={i}>{area.name || `Area ${i + 1}`}{area.default ? " (default)" : ""}</option>)}
      </select>
    </div>
    <div className="descene-actions">
      <button type="button" onClick={() => add()}>Add spawn area</button>
      <button type="button" disabled={!area} onClick={() => add(area)}>Duplicate spawn area</button>
      <button type="button" disabled={!area} onClick={remove}>Delete spawn area</button>
    </div>
    {area ? <>
      {field("name", "Spawn area name")}
      <label className="descene-check"><input type="checkbox" checked={area.default} onChange={event => onChange(areas.map((area, i) => ({ ...area, default: i === index && event.target.checked })))} />Default spawn area</label>
      <p>Position in meters. Use two numbers separated by a comma for a range.</p>
      <div className="descene-coordinates">{field("x", "Spawn X")}{field("y", "Spawn Y")}{field("z", "Spawn Z")}</div>
      <p>Camera target in meters. Leave all three blank to use the scene default.</p>
      <div className="descene-coordinates">{field("cameraX", "Camera target X")}{field("cameraY", "Camera target Y")}{field("cameraZ", "Camera target Z")}</div>
    </> : <p>Add an area to choose where visitors arrive.</p>}
  </section>;
}
