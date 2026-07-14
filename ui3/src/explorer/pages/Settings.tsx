import type { ReactNode } from "react";
import { memo, useEffect, useState } from "react";
import ExploreChrome, { type TabId } from "../frames/ExploreChrome";
import Toggle from "../../atoms/Toggle";
import Slider from "../../atoms/Slider";
import Dropdown from "../../components/Dropdown";
import { sendBridge } from "../../overlay/bridge";
import "./settings.css";

// Our bridge has no settings-read path (no GetSettings/Setting type) — fields render
// from the static CONTENT catalog below, seeded from bevy-explorer's real
// system_bridge::settings defaults (crates/system_bridge/src/settings/*.rs), and only
// ever write forward via sendBridge('SetSetting', ...).
type PillId = "graphics" | "sounds" | "controls";

type NamedVariant = { label: string; value: number };

type SettingModule = {
  title: string;
  setting?: string;
  fullscreen?: boolean;
  namedVariants?: NamedVariant[];
  min?: number;
  max?: number;
  step?: number;
  scale?: number;
  defaultValue?: number;
  format?: (v: number) => number | string;
};
type SettingGroup = { title: string; modules: SettingModule[] };

const pct = (v: number): string => Math.round(v) + "%";

function humanize(s: string): string {
  return s.replace(/[_-]+/g, " ").replace(/\b\w/g, (c) => c.toUpperCase());
}

function isBinary(m: SettingModule): boolean {
  const variants = m.namedVariants?.length ?? 0;
  if (variants > 0) return variants === 2;
  return m.min !== undefined && m.max !== undefined && m.max - m.min <= 1 && (m.step ?? 1) >= 1;
}
function isSlider(m: SettingModule): boolean {
  return !isBinary(m) && (m.namedVariants?.length ?? 0) <= 2;
}

function engineDefault(m: SettingModule): number {
  return m.defaultValue ?? m.namedVariants?.[0]?.value ?? 0;
}

function emitSetting(m: SettingModule, value: number): void {
  if (!m.setting) return;
  sendBridge("SetSetting", { name: m.setting, value });
}

function toggleFullscreen(on: boolean): void {
  try {
    if (on) document.documentElement.requestFullscreen?.();
    else if (document.fullscreenElement) document.exitFullscreen?.();
  } catch {
  }
}

// The engine has its own WindowSetting for native fullscreen, but on the web target we
// stay on the DOM Fullscreen API directly — it's the actual source of truth for the
// canvas, and the toggle needs to reflect exits the user triggers outside our control
// (Escape, F11) rather than just the state of its own last click.
function useIsFullscreen(): boolean {
  const [on, setOn] = useState(() => !!document.fullscreenElement);
  useEffect(() => {
    const sync = () => setOn(!!document.fullscreenElement);
    document.addEventListener("fullscreenchange", sync);
    document.addEventListener("fullscreenerror", sync);
    return () => {
      document.removeEventListener("fullscreenchange", sync);
      document.removeEventListener("fullscreenerror", sync);
    };
  }, []);
  return on;
}

function FullscreenToggle({ title }: { title: string }) {
  const on = useIsFullscreen();
  return <Toggle ariaLabel={title} checked={on} onChange={toggleFullscreen} />;
}

function Control({ m }: { m: SettingModule }) {
  if (m.fullscreen) {
    return <FullscreenToggle title={m.title} />;
  }
  if (isBinary(m)) {
    const variants = m.namedVariants ?? [{ label: "Off", value: 0 }, { label: "On", value: 1 }];
    const on = variants[1]!.value;
    const off = variants[0]!.value;
    return (
      <Toggle
        ariaLabel={m.title}
        defaultChecked={engineDefault(m) === on}
        onChange={(c) => emitSetting(m, c ? on : off)}
      />
    );
  }
  if (isSlider(m)) {
    const scale = m.scale ?? 1;
    return (
      <Slider
        ariaLabel={m.title}
        defaultValue={engineDefault(m) / scale}
        min={m.min}
        max={m.max}
        step={m.step}
        format={m.format}
        onChange={(v) => emitSetting(m, v * scale)}
      />
    );
  }
  const variants = m.namedVariants ?? [];
  const current = engineDefault(m);
  return (
    <Dropdown
      ariaLabel={m.title}
      options={variants.map((v) => v.label)}
      defaultValue={variants.find((v) => v.value === current)?.label}
      onChange={(label) => {
        const picked = variants.find((v) => v.label === label);
        if (picked) emitSetting(m, picked.value);
      }}
    />
  );
}

const SettingField = memo(function SettingField({ m }: { m: SettingModule }) {
  return (
    <div className="set__module">
      <div className="set__modtitle">{m.title}</div>
      <div className="set__modctl">
        <Control m={m} />
      </div>
    </div>
  );
});

const PILL_ICONS: Record<PillId, ReactNode> = {
  graphics: (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <rect x="2.5" y="5" width="19" height="12" rx="2" />
      <path d="M8 21h8M12 17v4" />
    </svg>
  ),
  sounds: (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <path d="M4 9v6h4l5 4V5L8 9H4Z" />
      <path d="M16.5 8.5a5 5 0 0 1 0 7" />
    </svg>
  ),
  controls: (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <rect x="2.5" y="7" width="19" height="10" rx="5" />
      <path d="M7 10v4M5 12h4M15.5 11h.01M18 13h.01" />
    </svg>
  ),
};

const CONTENT: Record<PillId, SettingGroup[]> = {
  graphics: [
    { title: "Display", modules: [
      { title: "Fullscreen", fullscreen: true },
      { title: "FPS Limit", setting: "Target Frame Rate", defaultValue: 4,
        namedVariants: [{ label: "30", value: 3 }, { label: "60", value: 4 }, { label: "120", value: 5 }, { label: "144", value: 6 }, { label: "Unlimited", value: 7 }] },
    ]},
    { title: "Post Processing", modules: [
      { title: "Anti-aliasing", setting: "Anti-aliasing", defaultValue: 2,
        namedVariants: [{ label: "Off", value: 0 }, { label: "FXAA Low", value: 1 }, { label: "FXAA High", value: 2 }] },
      { title: "Bloom", setting: "Bloom", defaultValue: 2,
        namedVariants: [{ label: "Off", value: 0 }, { label: "On", value: 2 }] },
      { title: "Depth of Field", setting: "Depth of Field", defaultValue: 2,
        namedVariants: [{ label: "Off", value: 0 }, { label: "Low", value: 1 }, { label: "High", value: 2 }] },
      { title: "Fog", setting: "Fog", defaultValue: 2,
        namedVariants: [{ label: "Off", value: 0 }, { label: "Basic", value: 1 }, { label: "Atmospheric", value: 2 }] },
    ]},
    { title: "Lighting", modules: [
      { title: "Ambient brightness", setting: "Ambient Brightness", defaultValue: 50, format: pct },
    ]},
    { title: "Shadows", modules: [
      { title: "Shadow quality", setting: "Shadow settings", defaultValue: 2,
        namedVariants: [{ label: "Off", value: 0 }, { label: "Low", value: 1 }, { label: "High", value: 2 }] },
      { title: "Shadow distance", setting: "Shadow Distance", scale: 3, defaultValue: 100, format: pct },
    ]},
  ],
  sounds: [
    { title: "Volume", modules: [
      { title: "Master volume", setting: "Master Volume", defaultValue: 100, format: pct },
      { title: "World sounds", setting: "Scene Volume", defaultValue: 100, format: pct },
      { title: "Avatar sounds", setting: "Avatar Volume", defaultValue: 100, format: pct },
      { title: "UI sounds", setting: "System Volume", defaultValue: 100, format: pct },
    ]},
    { title: "Voice Chat", modules: [
      { title: "Voice chat volume", setting: "Voice Volume", defaultValue: 100, format: pct },
    ]},
  ],
  controls: [
    { title: "Mouse", modules: [
      { title: "Look sensitivity", setting: "Pointer and Locked Camera sensitivity", min: 1, defaultValue: 50, format: pct },
      { title: "Camera sensitivity", setting: "Camera Sensitivity", min: 1, defaultValue: 50, format: pct },
    ]},
    { title: "Point At", modules: [
      { title: "Point-at marker", setting: "Point at marker visibility", defaultValue: 0,
        namedVariants: [{ label: "Everyone", value: 0 }, { label: "Friends", value: 1 }, { label: "Off", value: 2 }] },
    ]},
  ],
};

const CATEGORIES = Object.keys(CONTENT) as PillId[];

export default function Settings() {
  const [tab, setTab] = useState<TabId>("settings");
  const [pill, setPill] = useState<PillId>("graphics");
  const [resetEpoch, setResetEpoch] = useState(0);
  const groups = CONTENT[pill];

  function resetActiveTab(): void {
    for (const g of groups) {
      for (const m of g.modules) {
        if (m.fullscreen || !m.setting) continue;
        emitSetting(m, engineDefault(m));
      }
    }
    setResetEpoch((e) => e + 1);
  }

  return (
    <ExploreChrome active={tab} onTab={setTab}>
      <div className="set">
        <div className="set__head">
          <h1 className="set__title">Settings</h1>
          <div className="set__pills" role="tablist" aria-label="Settings sections">
            {CATEGORIES.map((c) => (
              <button
                key={c}
                type="button"
                role="tab"
                aria-selected={c === pill}
                className={"set__pill" + (c === pill ? " is-active" : "")}
                onClick={() => setPill(c)}
              >
                <span className="set__pillicon">{PILL_ICONS[c]}</span>
                {humanize(c)}
              </button>
            ))}
          </div>
          <button type="button" className="set__reset" onClick={resetActiveTab}>
            ↺ Reset all defaults
          </button>
        </div>

        <div className="set__card">
          <div className="set__content">
            {groups.length === 0 && (
              <div className="set__empty">No settings in this section yet.</div>
            )}
            {groups.map((g) => (
              <section className="set__group" key={g.title}>
                <h2 className="set__grouptitle">{g.title}</h2>
                <div className="set__modules">
                  {g.modules.map((m) => (
                    <SettingField key={`${m.setting ?? m.title}-${resetEpoch}`} m={m} />
                  ))}
                </div>
              </section>
            ))}
          </div>
        </div>
      </div>
    </ExploreChrome>
  );
}
