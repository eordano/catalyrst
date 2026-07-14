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

/** The key list is picked by name: `populated` is the demo set, `empty` the zero-state. */
const KEY_SETS = { populated: SCENE_KEYS, empty: [] as SceneKey[] };
type KeySetName = keyof typeof KEY_SETS;

type SceneProps = ComponentProps<typeof StStorageScene>;

/** Story args: the key list is picked by name, everything else is a real prop. */
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
  // The page latches `sceneKeys` and `initialDialog` into useState on mount, so those controls
  // would look dead without a key derived from them -- changing the arg re-renders the same
  // instance, which keeps its first row set and its first dialog state. The key forces a remount.
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

/**
 * Every state rendered at once, dialogs included. `Default` flips between them with the
 * `keySet` / `loading` / `initialDialog` controls; this story keeps the populated table, the
 * empty state, the loading skeleton and both dialogs in the render + a11y + visual-diff gates.
 * The dialog entries pass `portal={false}`, which lays the same `Modal` card out in normal
 * document flow instead of `createPortal`ing a `position: fixed; inset: 0` backdrop onto
 * `document.body` -- portalled dialogs stack on one another, so a single screenshot would
 * capture only the topmost. The heading ids are `useId()`-generated, so two dialogs on one
 * page do not collide.
 */
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
