import type { ComponentProps } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import StStorageScene from "./StStorageScene";
import type { SceneKey } from "./StStorageScene";

const SCENE_KEYS: SceneKey[] = [
  { key: "highScore" },
  { key: "puzzle.state" },
  { key: "doorUnlocked" },
  { key: "npc.dialogueProgress" },
  { key: "lastVisited" },
  { key: "collectedItems" },
  { key: "settings.musicVolume" },
];

const KEY_SETS = { populated: SCENE_KEYS, empty: [] as SceneKey[] };
type KeySetName = keyof typeof KEY_SETS;

type SceneProps = ComponentProps<typeof StStorageScene>;

type SceneStoryArgs = Omit<SceneProps, "sceneKeys"> & { keySet: KeySetName };

const meta = {
  title: "Web/Pages/Storage/Scene",
  component: StStorageScene,
  parameters: { layout: "fullscreen" },
  argTypes: {
    keySet: {
      control: "inline-radio",
      options: ["populated", "empty"],
      description: "Which `sceneKeys` list is rendered \u{2014} `empty` is the zero-state.",
    },
    loading: { control: "boolean" },
    realm: { control: "text" },
    position: { control: "text" },
    initialDialog: { control: "select", options: ["add", "edit"] },
    embedded: { control: "boolean" },
  },
  args: {
    keySet: "populated",
    loading: false,
    realm: "main",
    position: "-9,-9",
    initialDialog: null,
  },
  render: ({ keySet, initialDialog, ...rest }) => (
    <StStorageScene
      key={`${keySet}-${initialDialog}`}
      sceneKeys={KEY_SETS[keySet]}
      initialDialog={initialDialog}
      {...rest}
    />
  ),
} satisfies Meta<SceneStoryArgs>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const Catalog: Story = {
  name: "Catalog (every state)",
  parameters: {
    controls: { disable: true },
  },
  render: () => (
    <div className="st ui2" style={{ display: "flex", flexDirection: "column", gap: 48 }}>
      {/* <section> demotes each entry's unnamed header/footer/aside to `generic`
          (HTML-AAM scoped mapping) so the stack does not invent extra landmarks. */}
      <section>
        <div>populated</div>
        <StStorageScene sceneKeys={SCENE_KEYS} realm="main" position="-9,-9" chrome={false} />
      </section>
      <section>
        <div>empty</div>
        <StStorageScene sceneKeys={[]} realm="main" position="-9,-9" chrome={false} />
      </section>
      <section>
        <div>loading</div>
        <StStorageScene loading realm="main" position="-9,-9" chrome={false} />
      </section>
      <section>
        <div>add dialog</div>
        <StStorageScene
          sceneKeys={SCENE_KEYS}
          realm="main"
          position="-9,-9"
          chrome={false}
          initialDialog="add"
          portal={false}
        />
      </section>
      <section>
        <div>edit dialog</div>
        <StStorageScene
          sceneKeys={SCENE_KEYS}
          realm="main"
          position="-9,-9"
          chrome={false}
          initialDialog="edit"
          portal={false}
        />
      </section>
    </div>
  ),
};
