import type { Meta, StoryObj } from "@storybook/react-vite";
import MkNamesPage, { type MkNameStatus } from "./MkNamesPage";

const CREDITS_NOTE =
  "Credits can't be used for NAMEs yet \u{2014} Credits checkout only supports collection items.";

/** Every `MkNameStatus` shape, picked by name -- plus the query that produces it. */
const STATUSES = {
  idle: { status: { kind: "idle" }, value: "" },
  checking: { status: { kind: "checking" }, value: "aurora" },
  invalid: {
    status: { kind: "invalid", message: "NAMEs can't contain spaces." },
    value: "au rora",
  },
  claimable: {
    status: { kind: "claimable", priceMana: "100" },
    value: "aurora",
    creditsNote: CREDITS_NOTE,
  },
  listed: {
    status: { kind: "listed", name: "Automotive", priceMana: "5,000,000" },
    value: "automotive",
    creditsNote: CREDITS_NOTE,
  },
  taken: { status: { kind: "taken", name: "WOTC" }, value: "wotc" },
  error: {
    status: {
      kind: "error",
      message: "Couldn't check availability right now. Please try again.",
    },
    value: "aurora",
  },
} satisfies Record<string, { status: MkNameStatus; value: string; creditsNote?: string }>;

type StatusKey = keyof typeof STATUSES;
const STATUS_KEYS = Object.keys(STATUSES) as StatusKey[];

/** `statusKind` names the descriptor; `value` / `creditsNote` stay real props you can override. */
type NamesStoryArgs = {
  statusKind: StatusKey;
  value: string;
  creditsNote: string;
  maxLength: number;
};

const meta = {
  title: "Marketplace/Pages/Names",
  component: MkNamesPage,
  parameters: { layout: "fullscreen" },
  argTypes: {
    statusKind: { control: "select", options: STATUS_KEYS },
    value: { control: "text" },
    creditsNote: { control: "text" },
    maxLength: { control: "number" },
  },
  args: { statusKind: "claimable", value: "aurora", creditsNote: CREDITS_NOTE, maxLength: 15 },
  render: ({ statusKind, ...rest }) => (
    <MkNamesPage {...rest} status={STATUSES[statusKind].status} />
  ),
} satisfies Meta<NamesStoryArgs>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

/**
 * Kept as its own export rather than folded into `Default`: the story id
 * `marketplace-pages-names--idle` is a screen-tour deep link (`tools/screen-tour/add-story-links.mts`).
 */
export const Idle: Story = {
  args: { statusKind: "idle", value: "", creditsNote: "" },
};

/**
 * Every status at once, with the query that produces it. `Default` flips between them from the
 * Controls panel; this story keeps all seven in the render + a11y + visual-diff gates, since each
 * status renders a different verdict subtree. `chrome={false}` because stacking N copies of
 * `MarketplaceChrome` would emit N `<main>` landmarks and fail axe's landmark-unique.
 */
export const Catalog: Story = {
  name: "Catalog (every status)",
  parameters: { controls: { disable: true } },
  render: () => (
    <div style={{ display: "flex", flexDirection: "column", gap: 32, padding: 24 }}>
      {STATUS_KEYS.map((key) => (
        <section key={key}>
          <div style={{ font: "600 13px var(--font-sans)", opacity: 0.7, margin: "0 0 8px" }}>
            {key}
          </div>
          <MkNamesPage chrome={false} {...STATUSES[key]} />
        </section>
      ))}
    </div>
  ),
};
