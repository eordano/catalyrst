import { useEffect, useMemo, useRef, useState } from "react";
import Slider from "../../atoms/Slider";
import Checkbox from "../../atoms/Checkbox";
import "./derenderpanel.css";

/** Live render-tuning pane: every control drives one of the engine's console
    commands (crates/visuals/src/lib.rs) through the same-origin viewport
    iframe, so a drag shows up in the frame immediately. Nothing here is
    persisted into the scene -- it is a lens for finding values, and the
    Copy button emits the equivalent console commands so a chosen look can
    be replayed or committed as engine defaults. */

export const TONEMAP_MODES = [
  "none",
  "reinhard",
  "reinhard_luma",
  "aces",
  "agx",
  "sbdt",
  "tmmf",
  "blender",
] as const;

interface RenderState {
  tonemap: string;
  exposure: number;
  gamma: number;
  saturation: number;
  bloom: number;
  shadows: boolean;
  fog: boolean;
  timeOfDay: number;
  cloudCover: number;
  cloudDensity: number;
  cloudShadow: number;
  cloudScale: number;
  cloudSteps: number;
  cloudLacunarity: number;
  dofEnabled: boolean;
  dofFocalExtra: number;
  dofSensorHeight: number;
  dofFstops: number;
  dofMaxCircle: number;
  dofMaxDepth: number;
  dofBokeh: boolean;
  grassLayers: number;
  grassSubdivisions: number;
  grassDisplacement: number;
  grassRootColor: string;
  grassTipColor: string;
}

/** Engine defaults, mirrored from the command handlers' initial state. These
    seed the controls; the engine is only written when a control changes.
    Cloud values mirror the CloudCover insert_resource in visuals/lib.rs; DoF
    mirrors DofSetting::High; grass mirrors ParcelGrassConfig::default(). */
export const RENDER_DEFAULTS: RenderState = {
  tonemap: "blender",
  exposure: 0,
  gamma: 1,
  saturation: 1,
  bloom: 0.15,
  shadows: true,
  fog: true,
  timeOfDay: 12,
  cloudCover: 0.35,
  cloudDensity: 0.8,
  cloudShadow: 0.05,
  cloudScale: 1.5,
  cloudSteps: 44,
  cloudLacunarity: 2,
  dofEnabled: true,
  dofFocalExtra: 50,
  dofSensorHeight: 0.06,
  dofFstops: 0.05,
  dofMaxCircle: 20,
  dofMaxDepth: 250,
  dofBokeh: false,
  grassLayers: 32,
  grassSubdivisions: 32,
  grassDisplacement: 0.01,
  grassRootColor: "#3f6212",
  grassTipColor: "#65a30d",
};

/** The console line each field maps to; also what Copy settings emits.
    Multi-argument commands (dof, grass) collapse onto one key so a change to
    any of their fields re-sends the whole command. */
export function commandFor(key: keyof RenderState, s: RenderState): string {
  switch (key) {
    case "tonemap":
      return `/tonemap ${s.tonemap}`;
    case "exposure":
      return `/exposure ${round2(s.exposure)}`;
    case "gamma":
      return `/gamma ${round2(s.gamma)}`;
    case "saturation":
      return `/saturation ${round2(s.saturation)}`;
    case "bloom":
      return `/bloom ${round2(s.bloom)}`;
    case "shadows":
      return `/shadows ${s.shadows}`;
    case "fog":
      return `/fog ${s.fog}`;
    case "timeOfDay":
      return `/time ${round2(s.timeOfDay)} 0`;
    case "cloudCover":
      return `/cloud ${round2(s.cloudCover)}`;
    case "cloudDensity":
      return `/clouddensity ${round2(s.cloudDensity)}`;
    case "cloudShadow":
      return `/cloudshadow ${round2(s.cloudShadow)}`;
    case "cloudScale":
      return `/cloudscale ${round2(s.cloudScale)}`;
    case "cloudSteps":
      return `/cloudsteps ${Math.round(s.cloudSteps)}`;
    case "cloudLacunarity":
      return `/cloudlacunarity ${round2(s.cloudLacunarity)}`;
    case "dofEnabled":
    case "dofFocalExtra":
    case "dofSensorHeight":
    case "dofFstops":
    case "dofMaxCircle":
    case "dofMaxDepth":
    case "dofBokeh":
      // A single engine command owns all DoF fields. There is no off switch
      // on /dof itself; "off" approximates it by opening the aperture wide.
      return `/dof ${round2(s.dofFocalExtra)} ${s.dofSensorHeight} ${
        s.dofEnabled ? s.dofFstops : 9999
      } ${round2(s.dofMaxCircle)} ${round2(s.dofMaxDepth)} ${s.dofBokeh ? 1 : 0}`;
    case "grassLayers":
    case "grassSubdivisions":
    case "grassDisplacement":
    case "grassRootColor":
    case "grassTipColor":
      return `/grass ${Math.round(s.grassLayers)} ${Math.round(s.grassSubdivisions)} ${
        s.grassDisplacement
      } ${s.grassRootColor.replace("#", "")} ${s.grassTipColor.replace("#", "")}`;
  }
}

/** Copy/Reset iterate one representative key per engine command. */
const ORDERED_KEYS: Array<keyof RenderState> = [
  "tonemap",
  "exposure",
  "gamma",
  "saturation",
  "bloom",
  "shadows",
  "fog",
  "timeOfDay",
  "cloudCover",
  "cloudDensity",
  "cloudShadow",
  "cloudScale",
  "cloudSteps",
  "cloudLacunarity",
  "dofEnabled",
  "grassLayers",
];

function round2(v: number): number {
  return Math.round(v * 100) / 100;
}

interface RowProps {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  format?: (v: number) => string;
  onChange: (v: number) => void;
}

function SliderRow({ label, value, min, max, step, format, onChange }: RowProps) {
  return (
    <div className="erp-row">
      <span className="erp-label">{label}</span>
      <Slider
        value={value}
        min={min}
        max={max}
        step={step}
        ariaLabel={label}
        format={format ?? ((v) => String(round2(v)))}
        onChange={onChange}
      />
    </div>
  );
}

export interface DeRenderPanelProps {
  /** Runs one console line (leading slash included) against the engine. */
  onCommand?: (line: string) => void;
  /** Reads the engine's current render state back as one JSON string. Optional
      because the /renderstate command may not exist on every deployed engine;
      any failure keeps the seeded defaults. */
  onQueryState?: () => Promise<string>;
  onClose?: () => void;
  onOpenCameraSettings?: () => void;
}

export default function DeRenderPanel({
  onCommand,
  onQueryState,
  onClose,
  onOpenCameraSettings,
}: DeRenderPanelProps) {
  const [s, setS] = useState<RenderState>(RENDER_DEFAULTS);
  const [copied, setCopied] = useState(false);
  // One trailing timer per field: slider drags fire onChange per pixel, and
  // the engine only needs the value it settles on.
  const timers = useRef<Partial<Record<keyof RenderState, ReturnType<typeof setTimeout>>>>({});

  const apply = (key: keyof RenderState, next: RenderState) => {
    setS(next);
    const t = timers.current[key];
    if (t) clearTimeout(t);
    timers.current[key] = setTimeout(() => {
      delete timers.current[key];
      onCommand?.(commandFor(key, next));
    }, 120);
  };
  const set = <K extends keyof RenderState>(key: K, value: RenderState[K]) =>
    apply(key, { ...s, [key]: value });

  // Read the engine's live render state back on open so the pane shows what
  // the engine actually renders (including values set from the engine console
  // rather than this pane) instead of the seeds. One setS batch: sync sends no
  // commands, or every open would replay a full command burst at the engine.
  // The /renderstate command may not exist on the deployed engine yet; any
  // unknown-command reply or parse failure is swallowed and the seeds stand.
  const queryStateRef = useRef(onQueryState);
  queryStateRef.current = onQueryState;
  useEffect(() => {
    const query = queryStateRef.current;
    if (!query) return undefined;
    let disposed = false;
    void query()
      .then((reply) => {
        if (disposed) return;
        const data = JSON.parse(reply) as Record<string, unknown>;
        const num = (v: unknown): v is number => typeof v === "number" && Number.isFinite(v);
        const patch: Record<string, string | number | boolean> = {};
        if (
          typeof data.tonemap === "string" &&
          (TONEMAP_MODES as readonly string[]).includes(data.tonemap)
        ) {
          patch.tonemap = data.tonemap;
        }
        for (const key of ["exposure", "gamma", "saturation", "bloom"] as const) {
          if (num(data[key])) patch[key] = data[key] as number;
        }
        for (const key of ["shadows", "fog"] as const) {
          if (typeof data[key] === "boolean") patch[key] = data[key] as boolean;
        }
        const cloud = (data.cloud ?? {}) as Record<string, unknown>;
        if (num(cloud.cover)) patch.cloudCover = cloud.cover;
        if (num(cloud.density_cap)) patch.cloudDensity = cloud.density_cap;
        if (num(cloud.shadow)) patch.cloudShadow = cloud.shadow;
        if (num(cloud.scale)) patch.cloudScale = cloud.scale;
        if (num(cloud.steps)) patch.cloudSteps = cloud.steps;
        if (num(cloud.lacunarity)) patch.cloudLacunarity = cloud.lacunarity;
        const rawDof = data.dof;
        if (rawDof === null) {
          patch.dofEnabled = false;
        } else if (rawDof && typeof rawDof === "object") {
          patch.dofEnabled = true;
          const dof = rawDof as Record<string, unknown>;
          const pickNum = (names: string[], target: string) => {
            for (const n of names) {
              if (num(dof[n])) {
                patch[target] = dof[n] as number;
                break;
              }
            }
          };
          pickNum(["dofFocalExtra", "focal_extra"], "dofFocalExtra");
          pickNum(["dofSensorHeight", "sensor_height"], "dofSensorHeight");
          pickNum(["dofFstops", "f_stops", "fstops"], "dofFstops");
          pickNum(["dofMaxCircle", "max_circle"], "dofMaxCircle");
          pickNum(["dofMaxDepth", "max_depth"], "dofMaxDepth");
          const bokeh = dof.bokeh;
          if (typeof bokeh === "boolean") patch.dofBokeh = bokeh;
          else if (num(bokeh)) patch.dofBokeh = bokeh !== 0;
        }
        const grass = (data.grass ?? {}) as Record<string, unknown>;
        if (num(grass.layers)) patch.grassLayers = grass.layers;
        if (num(grass.subdivisions)) patch.grassSubdivisions = grass.subdivisions;
        if (num(grass.y_displacement)) patch.grassDisplacement = grass.y_displacement;
        if (Object.keys(patch).length === 0) return;
        setS((prev) => {
          const next = { ...prev };
          for (const [k, v] of Object.entries(patch)) {
            (next as unknown as Record<string, unknown>)[k] = v;
          }
          return next;
        });
      })
      .catch(() => {});
    return () => {
      disposed = true;
    };
  }, []);

  const script = useMemo(
    () => ORDERED_KEYS.map((k) => commandFor(k, s)).join("\n"),
    [s],
  );

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(script);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
    }
  };

  const resetAll = () => {
    setS(RENDER_DEFAULTS);
    for (const k of ORDERED_KEYS) onCommand?.(commandFor(k, RENDER_DEFAULTS));
  };

  return (
    <div className="erp" role="dialog" aria-label="Render tuning">
      <div className="erp-head">
        <span className="erp-title">Render tuning</span>
        <span className="erp-note">live, not saved into the scene</span>
        <button type="button" className="erp-x" aria-label="Close render tuning" onClick={onClose}>
          {"\u{2715}"}
        </button>
      </div>

      <div className="erp-section">Color</div>
      <label className="erp-row">
        <span className="erp-label">Tonemap</span>
        <select
          className="erp-select"
          aria-label="Tonemap"
          value={s.tonemap}
          onChange={(e) => set("tonemap", e.target.value)}
        >
          {TONEMAP_MODES.map((m) => (
            <option key={m} value={m}>
              {m}
            </option>
          ))}
        </select>
      </label>
      <SliderRow label="Exposure" value={s.exposure} min={-4} max={4} step={0.05} onChange={(v) => set("exposure", v)} />
      <SliderRow label="Gamma" value={s.gamma} min={0.25} max={2.5} step={0.05} onChange={(v) => set("gamma", v)} />
      <SliderRow label="Saturation" value={s.saturation} min={0} max={2} step={0.05} onChange={(v) => set("saturation", v)} />
      <SliderRow label="Bloom" value={s.bloom} min={0} max={1} step={0.01} onChange={(v) => set("bloom", v)} />

      <div className="erp-section">Environment</div>
      <SliderRow
        label="Time of day"
        value={s.timeOfDay}
        min={0}
        max={24}
        step={0.1}
        format={(v) => `${Math.floor(v)}:${String(Math.round((v % 1) * 60)).padStart(2, "0")}`}
        onChange={(v) => set("timeOfDay", v)}
      />
      <div className="erp-checks">
        <Checkbox checked={s.shadows} onChange={(next) => set("shadows", next)}>
          Shadows
        </Checkbox>
        <Checkbox checked={s.fog} onChange={(next) => set("fog", next)}>
          Fog
        </Checkbox>
      </div>

      <div className="erp-section">Clouds</div>
      <SliderRow label="Cover" value={s.cloudCover} min={0} max={1} step={0.01} onChange={(v) => set("cloudCover", v)} />
      <SliderRow label="Density" value={s.cloudDensity} min={0} max={2} step={0.05} onChange={(v) => set("cloudDensity", v)} />
      <SliderRow label="Shadow" value={s.cloudShadow} min={0} max={1} step={0.01} onChange={(v) => set("cloudShadow", v)} />
      <SliderRow label="Scale" value={s.cloudScale} min={0.25} max={4} step={0.05} onChange={(v) => set("cloudScale", v)} />
      <SliderRow label="Steps" value={s.cloudSteps} min={1} max={128} step={1} onChange={(v) => set("cloudSteps", v)} />
      <SliderRow
        label="Lacunarity"
        value={s.cloudLacunarity}
        min={1}
        max={4}
        step={0.05}
        onChange={(v) => set("cloudLacunarity", v)}
      />

      <div className="erp-section">Depth of field</div>
      <div className="erp-checks">
        <Checkbox checked={s.dofEnabled} onChange={(next) => set("dofEnabled", next)}>
          Enabled
        </Checkbox>
        <Checkbox checked={s.dofBokeh} onChange={(next) => set("dofBokeh", next)}>
          Bokeh
        </Checkbox>
      </div>
      <SliderRow
        label="Focal extra"
        value={s.dofFocalExtra}
        min={0}
        max={200}
        step={1}
        onChange={(v) => set("dofFocalExtra", v)}
      />
      <SliderRow
        label="F-stops"
        value={s.dofFstops}
        min={0.01}
        max={2}
        step={0.01}
        onChange={(v) => set("dofFstops", v)}
      />
      <SliderRow
        label="Max blur"
        value={s.dofMaxCircle}
        min={1}
        max={64}
        step={1}
        onChange={(v) => set("dofMaxCircle", v)}
      />
      <SliderRow
        label="Max depth"
        value={s.dofMaxDepth}
        min={10}
        max={1000}
        step={10}
        onChange={(v) => set("dofMaxDepth", v)}
      />

      <div className="erp-section">Grass</div>
      <SliderRow
        label="Layers"
        value={s.grassLayers}
        min={1}
        max={128}
        step={1}
        onChange={(v) => set("grassLayers", v)}
      />
      <SliderRow
        label="Subdivisions"
        value={s.grassSubdivisions}
        min={1}
        max={128}
        step={1}
        onChange={(v) => set("grassSubdivisions", v)}
      />
      <SliderRow
        label="Height"
        value={s.grassDisplacement}
        min={0}
        max={0.1}
        step={0.001}
        format={(v) => v.toFixed(3)}
        onChange={(v) => set("grassDisplacement", v)}
      />
      <label className="erp-row">
        <span className="erp-label">Root color</span>
        <input
          type="color"
          className="erp-color"
          aria-label="Grass root color"
          value={s.grassRootColor}
          onChange={(e) => set("grassRootColor", e.target.value)}
        />
      </label>
      <label className="erp-row">
        <span className="erp-label">Tip color</span>
        <input
          type="color"
          className="erp-color"
          aria-label="Grass tip color"
          value={s.grassTipColor}
          onChange={(e) => set("grassTipColor", e.target.value)}
        />
      </label>

      <div className="erp-actions">
        {onOpenCameraSettings && (
          <button type="button" className="erp-btn" onClick={onOpenCameraSettings}>
            Camera controls{"\u{2026}"}
          </button>
        )}
        <button type="button" className="erp-btn" onClick={resetAll}>
          Reset
        </button>
        <button
          type="button"
          className="erp-btn"
          title="Copy these settings as engine console commands"
          onClick={copy}
        >
          {copied ? "Copied" : "Copy settings"}
        </button>
      </div>
    </div>
  );
}
