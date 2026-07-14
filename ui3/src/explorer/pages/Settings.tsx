import type { ReactNode } from "react";
import { memo, useEffect, useState } from "react";
import Toggle from "../../atoms/Toggle";
import Slider from "../../atoms/Slider";
import Dropdown from "../../components/Dropdown";
import VoiceParticipantList from "../components/VoiceParticipantList";
import type { EngineSetting, EngineSettingInfo } from "../../overlay/engineSettings";
import type { SettingGroup, SettingModule, SettingsTab } from "../../data/settings/catalog";
import "./settings.css";

export type SettingsPanelProps = {
  tabs: SettingsTab[];
  tab: string;
  onTab: (id: string) => void;
  groups: SettingGroup[];
  info: EngineSettingInfo | null;
  values: Record<string, number>;
  defaults: Record<string, number>;
  onChange: (m: SettingModule, value: number) => void;
  onReset?: () => void;
  engineConnected?: boolean | null;
};

const PILL_ICONS: Record<string, ReactNode> = {
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
  chat: (
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <path d="M4 5h16a1 1 0 0 1 1 1v9a1 1 0 0 1-1 1H9l-4 4v-4H4a1 1 0 0 1-1-1V6a1 1 0 0 1 1-1Z" />
    </svg>
  ),
};

// The engine has its own WindowSetting for native fullscreen, but on the web
// target the DOM Fullscreen API is the actual source of truth for the canvas,
// and the toggle must reflect exits made outside our control (Escape, F11).
function useIsFullscreen(): boolean {
  const [on, setOn] = useState(
    () => typeof document !== "undefined" && !!document.fullscreenElement,
  );
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
  return (
    <Toggle
      ariaLabel={title}
      checked={on}
      onChange={(next: boolean) => {
        try {
          if (next) document.documentElement.requestFullscreen?.();
          else if (document.fullscreenElement) document.exitFullscreen?.();
        } catch {
        }
      }}
    />
  );
}

function formatFor(m: SettingModule): (v: number) => number | string {
  if (m.unit === "%") return (v) => Math.round(v) + "%";
  if (m.unit) return (v) => Math.round(v) + " " + m.unit;
  return (v) => Math.round(v);
}

type ModuleRowProps = {
  m: SettingModule;
  engine: EngineSetting | undefined;
  value: number;
  onValue: (value: number) => void;
};

const ModuleRow = memo(function ModuleRow({ m, engine, value, onValue }: ModuleRowProps) {
  const variants = engine?.namedVariants ?? [];
  const options = variants.length > 0 ? variants.map((v) => v.label) : m.options ?? [];
  const optionTitles = variants.length > 0 ? variants.map((v) => v.description) : undefined;
  const min = engine?.minValue ?? m.min ?? 0;
  const max = engine?.maxValue ?? m.max ?? 100;
  const step = engine?.stepSize ?? 1;
  const description = engine?.description ?? undefined;

  return (
    <div className="set__module">
      <div className="set__modtitle" title={description}>{m.title}</div>
      <div className="set__modctl">
        {m.fullscreen ? (
          <FullscreenToggle title={m.title} />
        ) : m.kind === "toggle" ? (
          <Toggle
            ariaLabel={m.title}
            checked={value >= 1}
            onChange={(next: boolean) => onValue(next ? 1 : 0)}
          />
        ) : m.kind === "slider" ? (
          <Slider
            ariaLabel={m.title}
            value={value}
            min={min}
            max={max}
            step={step}
            format={formatFor(m)}
            onCommit={onValue}
          />
        ) : (
          <Dropdown
            ariaLabel={m.title}
            options={options}
            optionTitles={optionTitles}
            value={options[value] ?? options[0] ?? ""}
            onChange={(label: string) => {
              const idx = options.indexOf(label);
              if (idx >= 0) onValue(idx);
            }}
          />
        )}
      </div>
    </div>
  );
});

export default function SettingsPanel({
  tabs,
  tab,
  onTab,
  groups,
  info,
  values,
  defaults,
  onChange,
  onReset,
  engineConnected,
}: SettingsPanelProps) {
  const activeLabel = tabs.find((t) => t.id === tab)?.label ?? tab;
  return (
    <div className="set">
      <div className="set__head">
        <h1 className="set__title">Settings</h1>
        <div className="set__pills" role="tablist" aria-label="Settings sections">
          {tabs.map((t) => (
            <button
              key={t.id}
              type="button"
              role="tab"
              aria-selected={t.id === tab}
              className={"set__pill" + (t.id === tab ? " is-active" : "")}
              onClick={() => onTab(t.id)}
            >
              <span className="set__pillicon">{PILL_ICONS[t.id]}</span>
              {t.label}
            </button>
          ))}
        </div>
        {onReset && (
          <button type="button" className="set__reset" onClick={onReset}>
            &#x21BA; Reset {activeLabel} defaults
          </button>
        )}
      </div>

      <div className="set__card">
        <div className="set__content">
          {engineConnected === false && (
            <div className="set__offline" role="note">
              Not connected to the engine &#x2014; changes won&apos;t be saved.
            </div>
          )}
          {groups.length === 0 && (
            <div className="set__empty">No settings in this section yet.</div>
          )}
          {groups.map((g, gi) => (
            <section className="set__group" key={g.title || gi}>
              {g.title && <h2 className="set__grouptitle">{g.title}</h2>}
              <div className="set__modules">
                {g.modules.map((m) => (
                  <ModuleRow
                    key={m.key}
                    m={m}
                    engine={m.setting ? info?.[m.setting] : undefined}
                    value={
                      (m.setting ? values[m.setting] : undefined) ??
                      (m.setting ? defaults[m.setting] : undefined) ??
                      m.default ??
                      0
                    }
                    onValue={(v) => onChange(m, v)}
                  />
                ))}
              </div>
            </section>
          ))}
          {tab === "sounds" && (
            <section className="set__group">
              <h2 className="set__grouptitle">Voice Chat &amp; Streams &#x2014; Participants</h2>
              <VoiceParticipantList />
            </section>
          )}
        </div>
      </div>
    </div>
  );
}
