import type { Meta, StoryObj } from "@storybook/react-vite";
import CliEscape from "./CliEscape";
import UnbuiltPanel from "./UnbuiltPanel";

const TODAY = {
  none: undefined,
  prose: "the headcounts above are the whole picture.",
  link: (
    <>
      The <code>&#x27F3;</code> button at the top of this page. See{" "}
      <a href="/creator-hub/data-sources">Data sources</a> for why.
    </>
  ),
  cli: (
    <CliEscape
      command={
        "# start your explorer with --mcp yourself first \u{2014} this does not launch it\ncurl -s http://127.0.0.1:8123/unity-explorer-mcp -d '{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\"params\":{\"name\":\"get_scene_logs\",\"arguments\":{}}}'"
      }
      explain={"dcl-scene-bots is an MCP client you run beside your own explorer. It catches TypeError \u{B7} ReferenceError \u{B7} Cannot read \u{B7} is not a function \u{B7} unhandled promise rejection."}
    />
  ),
};

type PanelStoryArgs = {
  title: string;
  why: string;
  today: keyof typeof TODAY;
};

const meta = {
  title: "CreatorHub/Components/UnbuiltPanel",
  component: UnbuiltPanel,
  parameters: {
    docs: {
      description: {
        component:
          "A capability with no backend. Renders `<section role=\"note\">` and has **no click surface** \u{2014} no button, no disabled control, no form, no link that could return a fake success. A disabled button teaches \u{201C}this will work once I'm signed in\u{201D}; a dashed note naming the missing service teaches the truth.",
      },
    },
  },
  argTypes: {
    title: { control: "text" },
    why: { control: "text" },
    today: {
      control: "inline-radio",
      options: Object.keys(TODAY),
      description:
        "What a creator can do instead, today: nothing, a sentence, a link, or a copyable command.",
    },
  },
  args: {
    title: "Sessions & retention",
    why: "Presence persists addresses (scene_occupancy.addresses) but its HTTP API returns counts only. The client half of session/retention analytics exists in this repo with a generated zod model and a drift gate; the server route /creators/me/scenes/stats 404s.",
    today: "prose",
  },
  render: ({ title, why, today }) => (
    <div style={{ maxWidth: 560 }}>
      <UnbuiltPanel title={title} why={why} today={TODAY[today]} />
    </div>
  ),
} satisfies Meta<PanelStoryArgs>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

/** Nothing helps yet, and the panel says so rather than inventing a workaround. */
export const NoWorkaround: Story = {
  args: {
    title: "Live 2-D scene state",
    why: "Nothing serves a scene's current entity or player state. Presence knows addresses and parcel coordinates internally; nothing exposes them.",
    today: "none",
  },
};

export const WithLink: Story = {
  args: {
    title: "Tell me when it changes",
    why: "catalyrst-notifications is email preferences (first_wear.rs, ports/email.rs) with zero parcel, land or scene references. There is nothing to subscribe to.",
    today: "link",
  },
};

export const WithCliEscape: Story = {
  args: {
    title: "Did it break?",
    why: "catalyrst-telemetry ingests Sentry-shaped events and groups them into issues, but every read is behind require_telemetry_admin and the data carries no scene or owner dimension. There is no query that means \u{201C}my world\u{201D}.",
    today: "cli",
  },
};
