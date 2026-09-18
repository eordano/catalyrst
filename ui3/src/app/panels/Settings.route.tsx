import { useCallback } from "react";

import { useNavigate, useSearchParams } from "react-router";

import SettingsPanel from "../../explorer/pages/Settings";
import { useEngineSettings } from "../../overlay/engineSettings";
import {
  defaultValues,
  groupsForTab,
  SETTINGS_CATALOG,
  type SettingModule,
} from "../../data/settings/catalog";

const TABS = [...SETTINGS_CATALOG.tabs, { id: "flags", label: "Feature Flags" }];
const DEFAULTS = defaultValues(SETTINGS_CATALOG);

export default function SettingsRoute() {
  const navigate = useNavigate();
  const [params, setParams] = useSearchParams();
  const tab = TABS.find(tab => tab.id === params.get("section"))?.id ?? TABS[0]!.id;
  const setTab = (id: string) => setParams(previous => { previous.set("section", id); return previous; }, { replace: true });
  const { info, values, setValue, connected } = useEngineSettings();
  const groups = groupsForTab(SETTINGS_CATALOG, tab);

  const onChange = useCallback(
    (m: SettingModule, value: number) => {
      if (m.setting) setValue(m.setting, value);
    },
    [setValue],
  );

  const onReset = useCallback(() => {
    for (const g of groups) {
      for (const m of g.modules) {
        if (m.fullscreen || !m.setting) continue;
        setValue(m.setting, info?.[m.setting]?.default ?? m.default ?? 0);
      }
    }
  }, [groups, info, setValue]);

  return (
    <SettingsPanel
      onClose={() => navigate("/")}
      tabs={TABS}
      tab={tab}
      onTab={setTab}
      groups={groups}
      info={info}
      values={values}
      defaults={DEFAULTS}
      onChange={onChange}
      onReset={onReset}
      engineConnected={connected}
    />
  );
}
