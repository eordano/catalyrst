import type { Meta, StoryObj } from "@storybook/react-vite";
import DclTopBar from "./DclTopBar";

const VARIANTS = ["default", "dao", "sites"] as const;

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

export const SignedOut: Story = { args: { signedIn: false } };

export const SignedIn: Story = { args: { signedIn: true, account: "0x9f3c\u{2026}7a21" } };

export const DaoVariant: Story = { args: { variant: "dao", active: "vote" } };

export const SitesVariant: Story = { args: { variant: "sites", active: "whatson" } };
