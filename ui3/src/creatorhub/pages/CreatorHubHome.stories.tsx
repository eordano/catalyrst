import type { ComponentProps } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import CreatorHubHome from "./CreatorHubHome";

type HomeProps = ComponentProps<typeof CreatorHubHome>;

const SCENES = [
  { id: "s1", title: "Genesis Plaza Remix", href: "/creator-hub/scene-editor?pointer=0%2C0" },
  { id: "s2", title: "My Gallery", href: "/creator-hub/scene-editor?pointer=12%2C-4" },
  { id: "s3", title: "Parkour Park", href: "/creator-hub/scene-editor?pointer=-30%2C55" },
];

/** The scene list is picked by name; every other prop is real. */
const SCENE_SETS = {
  none: [],
  three: SCENES,
} satisfies Record<string, NonNullable<HomeProps["scenes"]>>;

type SceneSet = keyof typeof SCENE_SETS;

type HomeStoryArgs = Omit<HomeProps, "scenes"> & { sceneSet: SceneSet };

const meta = {
  title: "CreatorHub/Pages/Home",
  component: CreatorHubHome,
  parameters: { layout: "fullscreen" },
  argTypes: {
    sceneSet: {
      control: "select",
      options: Object.keys(SCENE_SETS),
      description: "Which scene fixture the Scenes card lists. `none` is the empty state.",
    },
    signedIn: { control: "boolean" },
    committee: { control: "boolean", description: "Unset falls back to the chrome auth context." },
    account: { control: "text" },
    name: { control: "text" },
    scenesError: { control: "boolean", description: "Scenes card shows the data-layer outage." },
    rescoping: { control: "boolean", description: "Scenes card shows the loading spinner." },
    chrome: { control: "boolean", description: "`false` drops the Creator Hub rail + topbar." },
  },
  args: {
    sceneSet: "none",
    signedIn: false,
    account: "",
    name: "",
    scenesError: false,
    rescoping: false,
  },
  render: ({ sceneSet, ...rest }) => <CreatorHubHome scenes={SCENE_SETS[sceneSet]} {...rest} />,
} satisfies Meta<HomeStoryArgs>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

/** Kept as its own export: `creatorhub-pages-home--unauthenticated` is a screen-tour deep link. */
export const Unauthenticated: Story = {
  args: { signedIn: false, sceneSet: "none" },
};

const CASES: { label: string; args: Partial<HomeStoryArgs> }[] = [
  { label: "Signed out \u{B7} no scenes", args: { signedIn: false, sceneSet: "none" } },
  {
    label: "Signed in \u{B7} with scenes",
    args: {
      signedIn: true,
      account: "0x1234567890abcdef1234567890abcdef12345678",
      name: "Creator",
      sceneSet: "three",
    },
  },
  {
    label: "Committee \u{B7} with scenes",
    args: { signedIn: true, committee: true, sceneSet: "three" },
  },
  { label: "Scenes failed to load", args: { signedIn: true, scenesError: true } },
  { label: "Scenes loading", args: { signedIn: true, rescoping: true } },
];

/**
 * `chrome={false}` because the rail is a `<nav>` and the content well a `<main id="ch-main">`:
 * stacking framed instances gives axe duplicate landmarks and duplicate ids.
 */
export const Catalog: Story = {
  name: "Catalog (every state)",
  parameters: { controls: { disable: true } },
  render: (args) => (
    <div style={{ display: "flex", flexDirection: "column", gap: 32, padding: 24 }}>
      {CASES.map((c) => {
        const { sceneSet, ...rest } = { ...args, ...c.args };
        return (
          <section key={c.label} aria-label={c.label}>
            <div style={{ font: "600 13px var(--font-sans)", opacity: 0.7, margin: "0 0 8px" }}>
              {c.label}
            </div>
            <CreatorHubHome {...rest} chrome={false} scenes={SCENE_SETS[sceneSet]} />
          </section>
        );
      })}
    </div>
  ),
};
