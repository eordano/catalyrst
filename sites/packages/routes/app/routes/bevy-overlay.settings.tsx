import type { ReactNode } from "react";
import { useCallback, useEffect, useRef, useState } from "react";
import { useSearchParams } from "react-router";

import ExploreChrome from "@ui/explorer/frames/ExploreChrome";
import Toggle from "@ui/atoms/Toggle";
import Slider from "@ui/atoms/Slider";
import Dropdown from "@ui/components/Dropdown";
import "@ui/explorer/pages/settings.css";

import {
  loadSettingsCatalog,
  groupsForTab,
  defaultValues,
  parseTab,
  type SettingsCatalog,
  type SettingGroup,
  type SettingModule,
  type TabId,
} from "@data/lib/catalyst/overlay/settings";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { getBridge } from "@features/components/bevy-overlay/bridge";
import { track, trackExposure } from "@core/lib/telemetry/track";

import type { Route } from "./+types/bevy-overlay.settings";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "overlay/settings";

type SettingValue = string | number | boolean;

const FALLBACK: Assignment = {
  variant: "pill-tabs",
  flags: { showPillTabs: true, persistLocal: true },
  experimentKey: "cl_settings_panel",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const tab = parseTab(url.searchParams.get("tab"));

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  trackExposure({
    sid,
    story: STORY,
    variant: assignment.variant,
    experimentKey: assignment.experimentKey,
  });

  const catalog = loadSettingsCatalog();

  const payload = {
    sid,
    tab,
    catalog,
    defaults: defaultValues(catalog),
  };

  return wrap(payload);
}

export default function BevyOverlaySettings({ loaderData }: Route.ComponentProps) {
  const d = loaderData;
  return (
    <SettingsPanel
      sid={d.sid}
      tab={d.tab}
      catalog={d.catalog}
      defaults={d.defaults}
    />
  );
}

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

type PanelProps = {
  sid: string;
  tab: TabId;
  catalog: SettingsCatalog;
  defaults: Record<string, SettingValue>;
};

function SettingsPanel({ sid, tab, catalog, defaults }: PanelProps) {
  const [searchParams, setSearchParams] = useSearchParams();

  const [values, setValues] = useState<Record<string, SettingValue>>(defaults);
  useEffect(() => {
    const persisted = readPersisted(catalog.storageKey);
    if (persisted) setValues((v) => ({ ...v, ...persisted }));
  }, [catalog.storageKey]);

  const opened = useRef(false);
  useEffect(() => {
    if (opened.current) return;
    opened.current = true;
    track("cl_settings_opened", { tab }, { sid, story: STORY });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sid]);

  const onTab = useCallback(
    (next: TabId) => {
      if (next === tab) return;
      track("cl_settings_tab_changed", { tab: next }, { sid, story: STORY });
      setSearchParams(
        (prev) => {
          const params = new URLSearchParams(prev);
          params.set("panel", "settings");
          params.set("tab", next);
          return params;
        },
        { preventScrollReset: true },
      );
    },
    [tab, sid, setSearchParams],
  );

  const onChange = useCallback(
    (m: SettingModule, value: SettingValue) => {
      setValues((prev) => {
        const nextValues = { ...prev, [m.key]: value };
        writePersisted(catalog.storageKey, nextValues);
        pushToBridge(tab, m.key, value);
        return nextValues;
      });
      track(
        "cl_setting_changed",
        { tab, key: m.key, kind: m.kind, value },
        { sid, story: STORY },
      );
    },
    [tab, sid, catalog.storageKey],
  );

  const groups = groupsForTab(catalog, tab);

  return (
    <ExploreChrome active="settings" onTab={(t: string) => void t} onClose={() => {}}>
      <div className="set">
        <div className="set__head">
          <h1 className="set__title">Settings</h1>
          <div className="set__pills" role="tablist" aria-label="Settings sections">
            {catalog.tabs.map((p) => (
              <button
                key={p.id}
                type="button"
                role="tab"
                aria-selected={p.id === tab}
                className={"set__pill" + (p.id === tab ? " is-active" : "")}
                onClick={() => onTab(p.id as TabId)}
              >
                <span className="set__pillicon">{PILL_ICONS[p.id]}</span>
                {p.label}
              </button>
            ))}
          </div>
        </div>

        <div className="set__card">
          <div className="set__content">
            {groups.length === 0 && (
              <div className="set__empty">No settings in this section yet.</div>
            )}
            {groups.map((g: SettingGroup) => (
              <section className="set__group" key={g.title}>
                <h3 className="set__grouptitle">{g.title}</h3>
                <div className="set__modules">
                  {g.modules.map((m) => (
                    <Module
                      key={m.key}
                      m={m}
                      value={values[m.key]}
                      onChange={(v) => onChange(m, v)}
                    />
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

type ModuleProps = {
  m: SettingModule;
  value: SettingValue | undefined;
  onChange: (value: SettingValue) => void;
};

function Module({ m, value, onChange }: ModuleProps) {
  const fmt = m.unit === "%" ? pct : (v: number) => Math.round(v);
  return (
    <div className="set__module">
      <div className="set__modtitle">{m.title}</div>
      <div className="set__modctl">
        {m.kind === "toggle" && (
          <Toggle
            ariaLabel={m.title}
            checked={Boolean(value)}
            onChange={(next: boolean) => onChange(next)}
          />
        )}
        {m.kind === "slider" && (
          <Slider
            {...({
              value: typeof value === "number" ? value : Number(m.default),
              min: m.min,
              max: m.max,
              format: fmt,
              ariaLabel: m.title,
              onChange: (next: number) => onChange(next),
            } as unknown as React.ComponentProps<typeof Slider>)}
          />
        )}
        {m.kind === "dropdown" && (
          <Dropdown
            {...({
              options: m.options ?? [],
              value: typeof value === "string" ? value : String(m.default),
              onChange: (next: string) => onChange(next),
            } as unknown as React.ComponentProps<typeof Dropdown>)}
          />
        )}
      </div>
    </div>
  );
}

const pct = (v: number) => Math.round(v) + "%";

function readPersisted(key: string): Record<string, SettingValue> | null {
  if (typeof window === "undefined") return null;
  try {
    const raw = window.localStorage?.getItem(key);
    if (!raw) return null;
    const parsed = JSON.parse(raw);
    return parsed && typeof parsed === "object" ? parsed : null;
  } catch {
    return null;
  }
}

function writePersisted(key: string, values: Record<string, SettingValue>): void {
  if (typeof window === "undefined") return;
  try {
    window.localStorage?.setItem(key, JSON.stringify(values));
  } catch {
  }
}

function pushToBridge(section: string, key: string, value: SettingValue): void {
  const bridge = getBridge();
  if (!bridge) return;
  try {
    (bridge.send as (a: string, p?: Record<string, unknown>) => void)(
      "SetSetting",
      { section, key, value },
    );
  } catch {
  }
}
