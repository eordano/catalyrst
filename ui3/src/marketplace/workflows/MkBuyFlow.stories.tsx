import type { ComponentProps } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import MkBuyFlow from "./MkBuyFlow";

type BuyProps = ComponentProps<typeof MkBuyFlow>;

const SAMPLE_ASSET: NonNullable<BuyProps["asset"]> = {
  name: "Cyber Ronin Jacket",
  rarity: "legendary",
  network: "MATIC",
  kind: "wearable",
  priceMana: "1,250",
  priceUsd: "387.5000",
};

const STICKER_ASSET: NonNullable<BuyProps["asset"]> = {
  name: "Pixel Sticker",
  rarity: "common",
  network: "MATIC",
  kind: "emote",
  priceMana: "0.5",
  priceUsd: "0.1550",
};

const SAMPLE_COSTS = {
  tokenBalance: "2,480.55",
  itemCostToken: "1,250",
  itemCostUsd: "387.5000",
  feeCostToken: "0.0241",
  feeCostUsd: "0.0182",
  totalToken: "1,250",
  totalUsd: "387.5182",
  exchangeRate: "0.3100",
  duration: "Normal \u{2248} 20s",
};

/**
 * A fixture is the whole priced quote -- asset, chain, token and every cost line. The old variant
 * stories differed by which of these blobs they passed, so it becomes one named-preset arg.
 */
const FIXTURES = {
  manaPolygon: { asset: SAMPLE_ASSET, ...SAMPLE_COSTS },
  crossChain: {
    asset: SAMPLE_ASSET,
    ...SAMPLE_COSTS,
    crossChain: true,
    tokenSymbol: "USDC",
    chainName: "Ethereum",
    chainHue: 210,
    tokenBalance: "640.20",
    itemCostToken: "387.5000",
    itemCostUsd: "387.5000",
    feeCostToken: "4.21",
    feeCostUsd: "4.2100",
    totalToken: "391.71",
    totalUsd: "391.7100",
    exchangeRate: "3.2258",
    showFeeCovered: false,
  },
  usdcEthereum: {
    asset: SAMPLE_ASSET,
    ...SAMPLE_COSTS,
    tokenSymbol: "USDC",
    chainName: "Ethereum",
    chainHue: 210,
    showFeeCovered: false,
  },
  cheapEmote: {
    asset: STICKER_ASSET,
    ...SAMPLE_COSTS,
    itemCostToken: "0.5",
    itemCostUsd: "0.1550",
    totalToken: "0.5",
    totalUsd: "0.1550",
    showFeeCovered: false,
  },
} satisfies Record<string, Partial<BuyProps>>;
type FixtureName = keyof typeof FIXTURES;

const STATES = [
  "default",
  "card",
  "loadingRoute",
  "buying",
  "insufficient",
  "routeUnavailable",
  "priceTooLow",
] as const;
type BuyStateName = (typeof STATES)[number];

/** Story args: the quote is picked by fixture name, `state` is the real prop. */
type BuyStoryArgs = {
  fixture: FixtureName;
  state: BuyStateName;
};

const meta = {
  title: "Marketplace/Workflows/Buy",
  component: MkBuyFlow,
  parameters: { layout: "fullscreen" },
  argTypes: {
    fixture: {
      control: "select",
      options: ["manaPolygon", "crossChain", "usdcEthereum", "cheapEmote"],
      description:
        "Which priced quote the flow is confirming \u{2014} `crossChain` adds the duration/exchange-rate rows, `cheapEmote` is the sub-1-MANA item.",
    },
    state: {
      control: "select",
      options: [...STATES],
      description:
        "Which step or error the flow is in: `card` swaps in the Transak pane, `buying`/`loadingRoute` lock the primary button, and the last three each render a different warning.",
    },
  },
  args: { fixture: "manaPolygon", state: "default" },
  // The flow latches `state === "card"`, the chain and the token symbol into useState on mount,
  // so both controls would look dead without a key that remounts the component when they change.
  render: ({ fixture, state }) => (
    <MkBuyFlow key={`${fixture}-${state}`} state={state} {...FIXTURES[fixture]} />
  ),
} satisfies Meta<BuyStoryArgs>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

const CATALOG: { label: string; fixture: FixtureName; state: BuyStateName }[] = [
  { label: "default", fixture: "manaPolygon", state: "default" },
  { label: "cross-chain quote", fixture: "crossChain", state: "default" },
  { label: "loading route", fixture: "manaPolygon", state: "loadingRoute" },
  { label: "buying", fixture: "manaPolygon", state: "buying" },
  { label: "not enough MANA", fixture: "manaPolygon", state: "insufficient" },
  { label: "route unavailable", fixture: "usdcEthereum", state: "routeUnavailable" },
  { label: "price too low", fixture: "cheapEmote", state: "priceTooLow" },
  { label: "buy with card", fixture: "manaPolygon", state: "card" },
];

/**
 * Every step and error at once. This is possible because `Modal` takes `portal={false}`, which
 * lays the same card out in normal document flow instead of `createPortal`ing a
 * `position: fixed; inset: 0` backdrop onto `document.body` -- portalled dialogs stack on one
 * another, so a single screenshot would capture only the topmost. The flow renders no chrome, so
 * there is no `chrome={false}` to pass; each entry sits in a bare `<section>` so the stack does
 * not invent extra landmarks. The modal card's `<nav>` header is the one landmark a `<section>`
 * does NOT demote -- `nav` always maps to `navigation`, unlike an unnamed `aside`/`section` -- so
 * each entry also gets a `labelSuffix`, which names that `nav` per instance and keeps
 * `landmark-unique` green. Without it axe fails once per entry.
 */
export const Catalog: Story = {
  name: "Catalog (every state)",
  parameters: { controls: { disable: true } },
  render: () => (
    <div style={{ display: "flex", flexDirection: "column", gap: 24, padding: 24 }}>
      {CATALOG.map((entry) => (
        <section key={entry.label}>
          <div style={{ font: "600 13px var(--font-sans)", opacity: 0.7, margin: "0 0 8px" }}>
            {entry.label}
          </div>
          <MkBuyFlow
            portal={false}
            labelSuffix={`(${entry.label})`}
            state={entry.state}
            {...FIXTURES[entry.fixture]}
          />
        </section>
      ))}
    </div>
  ),
};
