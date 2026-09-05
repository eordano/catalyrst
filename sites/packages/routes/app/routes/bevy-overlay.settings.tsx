import { useCallback, useEffect, useRef } from "react";
import { useSearchParams } from "react-router";

import ExploreChrome from "@ui/explorer/frames/ExploreChrome";
import SettingsPanel from "@ui/explorer/pages/Settings";
import { useEngineSettings } from "@ui/overlay/engineSettings";
import type { SettingModule } from "@ui/data/settings/catalog";

import {
  loadSettingsCatalog,
  groupsForTab,
  defaultValues,
  parseTab,
  type SettingsCatalog,
  type TabId,
} from "@data/lib/catalyst/overlay/settings";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import type { Route } from "./+types/bevy-overlay.settings";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "overlay/settings";

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
    <SettingsRoute
      sid={d.sid}
      tab={d.tab}
      catalog={d.catalog}
      defaults={d.defaults}
    />
  );
}

type RouteProps = {
  sid: string;
  tab: TabId;
  catalog: SettingsCatalog;
  defaults: Record<string, number>;
};

function SettingsRoute({ sid, tab, catalog, defaults }: RouteProps) {
  const [, setSearchParams] = useSearchParams();
  const { info, values, setValue, connected } = useEngineSettings();

  const opened = useRef(false);
  useEffect(() => {
    if (opened.current) return;
    opened.current = true;
    track("cl_settings_opened", { tab }, { sid, story: STORY });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sid]);

  const onTab = useCallback(
    (raw: string) => {
      const next = parseTab(raw);
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
    (m: SettingModule, value: number) => {
      if (m.setting) setValue(m.setting, value);
      track(
        "cl_setting_changed",
        { tab, key: m.key, kind: m.kind, value },
        { sid, story: STORY },
      );
    },
    [tab, sid, setValue],
  );

  return (
    <ExploreChrome active="settings" onTab={(t: string) => void t} onClose={() => {}}>
      <SettingsPanel
        tabs={catalog.tabs}
        tab={tab}
        onTab={onTab}
        groups={groupsForTab(catalog, tab)}
        info={info}
        values={values}
        defaults={defaults}
        onChange={onChange}
        engineConnected={connected}
      />
    </ExploreChrome>
  );
}
