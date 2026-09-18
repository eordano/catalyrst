import { useEffect, useId, useState } from "react";
import { deviceEntityDepth, type DeviceTelemetry } from "../device-debug";
import "./dedevicetelemetry.css";

const metrics = [
  ["fps", "FPS", ""], ["draw_calls", "Draw calls", ""], ["primitives", "Primitives", ""],
  ["objects_in_frame", "Objects in frame", ""], ["mem_gpu_mb", "GPU memory", " MB"],
  ["mem_rust_mb", "Rust heap", " MB"], ["js_heap_used_mb", "JavaScript heap", " MB"],
  ["js_heap_total_mb", "JavaScript heap total", " MB"], ["assets_loading", "Assets loading", ""],
  ["assets_loaded", "Assets loaded", ""], ["download_speed_mbs", "Download speed", " MB/s"],
] as const;

export default function DeDeviceTelemetry({ data }: { data: DeviceTelemetry }) {
  const id = useId();
  const [selected, setSelected] = useState<string | null>(null);
  const [filter, setFilter] = useState("");
  const [scene, setScene] = useState("");
  useEffect(() => {
    if (selected && !data.entities[selected]) setSelected(null);
    if (scene && !Object.values(data.entities).some(entity => entity.scene === Number(scene))) setScene("");
  }, [data.entities, selected, scene]);
  const scenes = [...new Set(Object.values(data.entities).map(entity => entity.scene))].sort((a, b) => a - b);
  const entities = Object.entries(data.entities).filter(([, entity]) => (!scene || entity.scene === Number(scene)) && `${entity.id} ${Object.keys(entity.components).join(" ")}`.toLowerCase().includes(filter.toLowerCase()));
  const entity = selected && data.entities[selected];
  const maximum = Math.max(1, ...data.fps);
  const points = data.fps.map((value, index) => `${index * 300 / Math.max(1, data.fps.length - 1)},${60 - value * 58 / maximum}`).join(" ");
  return <section className="device-telemetry" aria-label="Device inspection">
    <details open><summary>Performance</summary>
      {data.performance ? <>
        <dl className="device-telemetry__metrics">{metrics.map(([key, label, unit]) => <div key={key}><dt>{label}</dt><dd>{data.performance?.[key] === undefined ? "Not reported" : `${data.performance[key]!.toLocaleString(undefined, { maximumFractionDigits: 1 })}${unit}`}</dd></div>)}</dl>
        {data.fps.length > 1 && <svg className="device-telemetry__chart" viewBox="0 0 300 64" role="img" aria-label={`FPS history, last ${data.fps.length} samples`}><polyline points={points} fill="none" stroke="currentColor" strokeWidth="2" /></svg>}
      </> : <p>Waiting for performance data from this device.</p>}
    </details>
    <details><summary>Entities and components</summary>
      <p>Shows entities observed in this debug session.</p>
      {data.limited && <p role="status">The 10,000-entity inspection limit was reached. Some entities are not shown.</p>}
      <div className="device-telemetry__filters">
        <label htmlFor={`${id}-scene`}><span id={`${id}-scene-label`}>Scene</span><select id={`${id}-scene`} aria-labelledby={`${id}-scene-label`} value={scene} onChange={event => { setScene(event.target.value); setSelected(null); }}><option value="">All scenes</option>{scenes.map(scene => <option key={scene} value={scene}>Scene {scene}</option>)}</select></label>
        <label htmlFor={`${id}-filter`}>Find entity or component<input id={`${id}-filter`} value={filter} onChange={event => setFilter(event.target.value)} /></label>
      </div>
      <div className="device-telemetry__split">
        <div role="list" aria-label="Observed entities" className="device-telemetry__entities">
          {entities.slice(0, 200).map(([key, entity]) => <div role="listitem" key={key}><button type="button" aria-pressed={selected === key} style={{ paddingInlineStart: 8 + deviceEntityDepth(entity, data.entities) * 12 }} onClick={() => setSelected(key)}>Scene {entity.scene} &#xb7; Entity {entity.id}</button></div>)}
          {entities.length > 200 && <p role="status">Showing the first 200 of {entities.length.toLocaleString()} matching entities. Refine the search to inspect others.</p>}
          {!entities.length && <p>No matching entities received.</p>}
        </div>
        <div className="device-telemetry__components">{entity ? <>
          <h3>Entity {entity.id}</h3><p>Scene {entity.scene} &#xb7; Parent {entity.parent}</p>
          {Object.entries(entity.components).map(([name, value]) => <details key={name}><summary>{name}</summary><pre>{JSON.stringify(value, null, 2)}</pre></details>)}
        </> : <p>Select an entity to inspect its components.</p>}</div>
      </div>
    </details>
  </section>;
}
