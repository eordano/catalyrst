import type { ComponentProps } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import MkSuccessPage from "./MkSuccessPage";

type Props = ComponentProps<typeof MkSuccessPage>;
type SuccessState = NonNullable<Props["state"]>;
type Asset = Props["asset"];

const ASSETS = {
  ens: { category: "ens", name: "myname", rarity: "rare" },
  wearable: { category: "wearable", name: "Cyber Jacket", rarity: "epic" },
  land: { category: "parcel", name: "-45,12", rarity: "legendary" },
  none: undefined,
} satisfies Record<string, Asset>;

type AssetKey = keyof typeof ASSETS;
const ASSET_KEYS = Object.keys(ASSETS) as AssetKey[];

const STATES = ["success", "loading", "error"] satisfies SuccessState[];

type SuccessStoryArgs = { state: SuccessState; assetKind: AssetKey };

const meta = {
  title: "Marketplace/Pages/Success",
  component: MkSuccessPage,
  parameters: { layout: "fullscreen" },
  argTypes: {
    state: { control: "select", options: STATES },
    assetKind: { control: "select", options: ASSET_KEYS },
  },
  args: { state: "success", assetKind: "ens" },
  render: ({ assetKind, ...rest }) => <MkSuccessPage {...rest} asset={ASSETS[assetKind]} />,
} satisfies Meta<SuccessStoryArgs>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

export const Catalog: Story = {
  name: "Catalog (every state)",
  parameters: { controls: { disable: true } },
  render: () => (
    <div style={{ display: "flex", flexDirection: "column", gap: 32, padding: 24 }}>
      {(
        [
          { label: "success \u{2014} ens", state: "success", assetKind: "ens" },
          { label: "success \u{2014} wearable", state: "success", assetKind: "wearable" },
          { label: "success \u{2014} land", state: "success", assetKind: "land" },
          { label: "loading", state: "loading", assetKind: "ens" },
          { label: "error", state: "error", assetKind: "none" },
        ] satisfies { label: string; state: SuccessState; assetKind: AssetKey }[]
      ).map((entry) => (
        <section key={entry.label}>
          <div style={{ font: "600 13px var(--font-sans)", opacity: 0.7, margin: "0 0 8px" }}>
            {entry.label}
          </div>
          <MkSuccessPage chrome={false} state={entry.state} asset={ASSETS[entry.assetKind]} />
        </section>
      ))}
    </div>
  ),
};
