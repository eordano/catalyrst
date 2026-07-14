import type { Meta, StoryObj } from "@storybook/react-vite";
import DclTopBar from "./DclTopBar";

/** Every chrome variant. */
const VARIANTS = ["default", "dao", "sites"] as const;

/** Union of every `LINK_DEFS` id, plus `""` for "no link highlighted". */
const NAV_IDS = ["", "explore", "whatson", "shop", "create", "learn", "vote", "events"] as const;

const meta = {
  title: "Web/Frames/DclTopBar",
  component: DclTopBar,
  parameters: { layout: "fullscreen" },
  argTypes: {
    variant: {
      control: "select",
      options: VARIANTS,
      description: "Which link set the bar renders.",
    },
    active: {
      control: "select",
      options: NAV_IDS,
      description: 'Which nav id is highlighted; `""` highlights none.',
    },
    signedIn: {
      control: "boolean",
      description: "Overrides the chrome auth context; unset defers to it.",
    },
    account: { control: "text" },
    transparent: { control: "boolean" },
    signInHref: { control: "text" },
  },
  args: { variant: "default", active: "shop" },
} satisfies Meta<typeof DclTopBar>;

export default meta;
type Story = StoryObj<typeof meta>;

/*
 * The four exports below are thin arg presets over the collapsed `meta` -- the variant space
 * itself now lives in `meta.argTypes`, so every combination is reachable from the Controls
 * panel. They keep their own exports (rather than folding into a single `Default`) because
 * `tools/screen-tour/add-story-links.mts` and the shipped `tools/screen-tour/tours/*` data
 * deep-link their story ids, and a missing id only console.warns. No state is dropped from
 * the render/axe/screenshot gates, so no `Catalog` is needed here.
 */

export const SignedOut: Story = { args: { signedIn: false } };

export const SignedIn: Story = { args: { signedIn: true, account: "0x9f3c\u{2026}7a21" } };

export const DaoVariant: Story = { args: { variant: "dao", active: "vote" } };

export const SitesVariant: Story = { args: { variant: "sites", active: "whatson" } };
