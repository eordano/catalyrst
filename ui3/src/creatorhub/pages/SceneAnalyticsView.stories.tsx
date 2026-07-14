import type { Meta, StoryObj } from "@storybook/react-vite";
import SceneAnalyticsView, {
  type SceneAnalyticsViewProps,
  type SceneSelection,
} from "./SceneAnalyticsView";
import {
  creatorScenesStatsFixture,
  FIXTURE_AS_OF,
  honestEmptyScene,
} from "../lib/scene-analytics.fixtures";

const SCENES = {
  portfolio: creatorScenesStatsFixture.scenes,
  honestEmpty: [honestEmptyScene],
  none: [],
} satisfies Record<string, SceneAnalyticsViewProps["scenes"]>;

const SELECTED = {
  none: null,
  genesisParcel: { sceneType: "genesis", sceneId: "-3|-2" },
  world: { sceneType: "world", sceneId: "kickoff.dcl.eth" },
  honestEmptyWorld: { sceneType: "world", sceneId: "sparse.dcl.eth" },
  missingWorld: { sceneType: "world", sceneId: "missing.dcl.eth" },
} satisfies Record<string, SceneSelection | null>;

type DatasetKey = keyof typeof SCENES;
type SelectionKey = keyof typeof SELECTED;

/** Story args: the scene fixture and the drilldown selection are picked by name. */
type AnalyticsStoryArgs = Omit<SceneAnalyticsViewProps, "scenes" | "selected"> & {
  dataset: DatasetKey;
  selection: SelectionKey;
};

const meta = {
  title: "CreatorHub/Pages/SceneAnalyticsView",
  component: SceneAnalyticsView,
  parameters: { layout: "fullscreen" },
  argTypes: {
    phase: { control: "inline-radio", options: ["signed-out", "loading", "error", "ready"] },
    dataset: {
      control: "select",
      options: Object.keys(SCENES),
      description: "Which scene-stats fixture backs the view. `none` is the empty portfolio.",
    },
    selection: {
      control: "select",
      options: Object.keys(SELECTED),
      description:
        "Drilldown target. `missingWorld` points at a scene absent from the dataset (no-data state).",
    },
    asOf: { control: "text" },
    error: { control: "text" },
    rankingEnabled: { control: "boolean" },
  },
  args: {
    phase: "ready",
    dataset: "portfolio",
    selection: "none",
    asOf: FIXTURE_AS_OF,
    worldAccess: { "kickoff.dcl.eth": "public" },
  },
  render: ({ dataset, selection, ...rest }) => (
    <SceneAnalyticsView {...rest} scenes={SCENES[dataset]} selected={SELECTED[selection]} />
  ),
} satisfies Meta<AnalyticsStoryArgs>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

const CASES: { label: string; args: Partial<AnalyticsStoryArgs> }[] = [
  { label: "Portfolio", args: {} },
  { label: "Drilldown \u{B7} Genesis parcel", args: { selection: "genesisParcel" } },
  { label: "Drilldown \u{B7} World", args: { selection: "world" } },
  {
    label: "Drilldown \u{B7} honest-empty World",
    args: { dataset: "honestEmpty", selection: "honestEmptyWorld" },
  },
  { label: "Drilldown \u{B7} no data for scene", args: { selection: "missingWorld" } },
  { label: "Empty portfolio", args: { dataset: "none", asOf: null } },
  { label: "Signed out", args: { phase: "signed-out" } },
  { label: "Loading", args: { phase: "loading" } },
  {
    label: "Load error",
    args: { phase: "error", error: "Failed to fetch scene metrics (status 502)" },
  },
];

export const Catalog: Story = {
  name: "Catalog (every phase and drilldown)",
  parameters: { controls: { disable: true } },
  render: (args) => (
    <div style={{ display: "flex", flexDirection: "column", gap: 40 }}>
      {CASES.map((c) => {
        const { dataset, selection, ...rest } = { ...args, ...c.args };
        return (
          <section key={c.label} aria-label={c.label}>
            <div style={{ font: "600 13px var(--font-sans)", opacity: 0.7, margin: "0 0 8px" }}>
              {c.label}
            </div>
            <SceneAnalyticsView
              {...rest}
              scenes={SCENES[dataset]}
              selected={SELECTED[selection]}
            />
          </section>
        );
      })}
    </div>
  ),
};
