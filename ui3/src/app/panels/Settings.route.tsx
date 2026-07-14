import { useCallback, useState } from "react";

import SettingsPanel from "../../explorer/pages/Settings";
import { useEngineSettings } from "../../overlay/engineSettings";
import {
  defaultValues,
  groupsForTab,
  SETTINGS_CATALOG,
  type SettingModule,
} from "../../data/settings/catalog";

const DEFAULTS = defaultValues(SETTINGS_CATALOG);

export default function SettingsRoute() {
  const [tab, setTab] = useState(SETTINGS_CATALOG.tabs[0]?.id ?? "graphics");
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
      tabs={SETTINGS_CATALOG.tabs}
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
