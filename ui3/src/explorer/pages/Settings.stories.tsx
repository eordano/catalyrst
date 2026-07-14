import type { Meta, StoryObj } from "@storybook/react-vite";
import { useState } from "react";

import SettingsPanel from "./Settings";
import type { EngineSettingInfo } from "../../overlay/engineSettings";
import {
  defaultValues,
  groupsForTab,
  SETTINGS_CATALOG,
} from "../../data/settings/catalog";

const DEFAULTS = defaultValues(SETTINGS_CATALOG);

const ENGINE_INFO: EngineSettingInfo = {
  Bloom: {
    name: "Bloom",
    category: "Graphics",
    minValue: 0,
    maxValue: 2,
    stepSize: 1,
    value: 1,
    default: 1,
    description: "Glow around bright light sources.",
    namedVariants: [
      { label: "Off", description: "No glow." },
      { label: "Low", description: "Subtle glow." },
      { label: "High", description: "Strong glow." },
    ],
  },
  "Shadow Distance": {
    name: "Shadow Distance",
    category: "Graphics",
    minValue: 0,
    maxValue: 300,
    stepSize: 1,
    value: 100,
    default: 100,
    description: "How far from the camera shadows are drawn.",
    namedVariants: [],
  },
};

function Harness({
  info,
  engineConnected,
}: {
  info: EngineSettingInfo | null;
  engineConnected: boolean | null;
}) {
  const [tab, setTab] = useState(SETTINGS_CATALOG.tabs[0]?.id ?? "graphics");
  const [values, setValues] = useState<Record<string, number>>({});
  return (
    <SettingsPanel
      tabs={SETTINGS_CATALOG.tabs}
      tab={tab}
      onTab={setTab}
      groups={groupsForTab(SETTINGS_CATALOG, tab)}
      info={info}
      values={values}
      defaults={DEFAULTS}
      onChange={(m, v) => {
        if (m.setting) setValues((prev) => ({ ...prev, [m.setting as string]: v }));
      }}
      engineConnected={engineConnected}
    />
  );
}

const meta = {
  title: "Explorer/Pages/Settings",
  component: SettingsPanel,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof SettingsPanel>;

export default meta;
// Render-only stories: the panel's props are all required, so args-typed
// StoryObj<typeof meta> would demand a full args block each story.
type Story = StoryObj;

export const Default: Story = {
  render: () => <Harness info={null} engineConnected={null} />,
};

export const EngineTooltips: Story = {
  render: () => <Harness info={ENGINE_INFO} engineConnected={true} />,
};

export const Bridgeless: Story = {
  render: () => <Harness info={null} engineConnected={false} />,
};
