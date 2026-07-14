import type { Meta, StoryObj } from "@storybook/react-vite";
import DeWorkspace from "./DeWorkspace";
import {
  GENESIS_PLAZA_TREE,
  GENESIS_PLAZA_INSPECTOR,
  GENESIS_PLAZA_CATALOG,
  GENESIS_PLAZA_TITLE,
} from "./genesisPlaza.fixture";
import { SPACE_INVADERS_FILES } from "./spaceInvaders.fixture";

const SCENE_TITLE = "Genesis Plaza Booth";

const SAMPLE_TREE = [
  { id: "512", name: "Ground", children: [] },
  {
    id: "513",
    name: "Kiosk",
    expanded: true,
    children: [
      { id: "514", name: "Counter", children: [] },
      { id: "515", name: "Sign", children: [] },
      { id: "516", name: "Spotlight", children: [] },
    ],
  },
  { id: "520", name: "Display Cube", selected: true, children: [] },
  { id: "521", name: "Ambient Audio", children: [] },
  { id: "530", name: "Spawn Point", children: [] },
];

const SAMPLE_INSPECTOR = { name: "Display Cube", id: "520" };

const SAMPLE_CATALOG = [
  { id: "a1", name: "Oak Tree", pack: "Nature", hue: 140 },
  { id: "a2", name: "Park Bench", pack: "Props", hue: 32 },
  { id: "a3", name: "Street Lamp", pack: "City", hue: 48 },
  { id: "a4", name: "Fountain", pack: "City", hue: 198 },
  { id: "a5", name: "Wooden Crate", pack: "Props", hue: 28 },
  { id: "a6", name: "Neon Sign", pack: "Decor", hue: 300 },
  { id: "a7", name: "Stone Arch", pack: "Structures", hue: 16 },
  { id: "a8", name: "Potted Palm", pack: "Nature", hue: 120 },
  { id: "a9", name: "Sci-Fi Door", pack: "Structures", hue: 220 },
];

const SAMPLE_LOCAL = [
  { path: "assets/models/booth.glb", folder: "models" },
  { path: "assets/models/banner.glb", folder: "models" },
  { path: "assets/podium.glb", folder: "" },
  { path: "assets/props/lamp.glb", folder: "props" },
];

const meta = {
  title: "Editor/Pages/Workspace",
  component: DeWorkspace,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof DeWorkspace>;
export default meta;

type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => (
    <DeWorkspace
      title={SCENE_TITLE}
      tree={SAMPLE_TREE}
      inspector={SAMPLE_INSPECTOR}
      catalog={SAMPLE_CATALOG}
      local={SAMPLE_LOCAL}
    />
  ),
};

export const WithAssets: Story = {
  render: () => (
    <DeWorkspace
      left="assets"
      title={SCENE_TITLE}
      tree={SAMPLE_TREE}
      inspector={SAMPLE_INSPECTOR}
      catalog={SAMPLE_CATALOG}
      local={SAMPLE_LOCAL}
    />
  ),
};

export const CodePanel: Story = {
  name: "Code panel (Space Invaders project)",
  render: () => (
    <DeWorkspace
      title="Space Invaders"
      tree={[{ id: "0", name: "Space Invaders", children: [] }]}
      inspector={{ name: "Space Invaders", id: "0" }}
      catalog={SAMPLE_CATALOG}
      code={{ virtualFiles: SPACE_INVADERS_FILES }}
    />
  ),
};

export const GenesisPlaza: Story = {
  name: "Genesis Plaza (captured real data)",
  render: () => (
    <DeWorkspace
      left="scene"
      title={GENESIS_PLAZA_TITLE}
      tree={GENESIS_PLAZA_TREE}
      inspector={GENESIS_PLAZA_INSPECTOR}
      catalog={GENESIS_PLAZA_CATALOG}
    />
  ),
};

export const GenesisPlazaAssets: Story = {
  name: "Genesis Plaza assets (captured real data)",
  render: () => (
    <DeWorkspace
      left="assets"
      title={GENESIS_PLAZA_TITLE}
      tree={GENESIS_PLAZA_TREE}
      inspector={GENESIS_PLAZA_INSPECTOR}
      catalog={GENESIS_PLAZA_CATALOG}
    />
  ),
};
