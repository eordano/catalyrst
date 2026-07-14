import type { ComponentProps } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import MkSettingsPage from "./MkSettingsPage";

type SellRows = NonNullable<ComponentProps<typeof MkSettingsPage>["selling"]>;

const SAMPLE_ADDRESS = "0x9f3c5b1a4d2e8f7c0b6a9e3d1f7a4c8b2e0d5a21";

const SELLING = {
  sample: [
    { id: "sell-mp-matic", contract: "Marketplace", token: "Wearables", network: "Polygon" },
    { id: "sell-mp-eth", contract: "Marketplace", token: "LAND", network: "Ethereum" },
  ],
  none: [],
} satisfies Record<string, SellRows>;

type SellingKey = keyof typeof SELLING;

/** `sellingFixture` names the authorization rows; everything else is a real prop. */
type SettingsStoryArgs = {
  address: string;
  isLoading: boolean;
  hasError: boolean;
  sellingFixture: SellingKey;
};

const meta = {
  title: "Marketplace/Pages/Settings",
  component: MkSettingsPage,
  parameters: { layout: "fullscreen" },
  argTypes: {
    address: { control: "text" },
    isLoading: { control: "boolean" },
    hasError: { control: "boolean" },
    sellingFixture: { control: "inline-radio", options: ["sample", "none"] },
  },
  args: { address: SAMPLE_ADDRESS, isLoading: false, hasError: false, sellingFixture: "sample" },
  render: ({ sellingFixture, ...rest }) => (
    <MkSettingsPage {...rest} selling={SELLING[sellingFixture]} />
  ),
} satisfies Meta<SettingsStoryArgs>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

const CASES: { label: string; args: Partial<SettingsStoryArgs> }[] = [
  { label: "Authorizations listed", args: {} },
  { label: "Loading", args: { isLoading: true } },
  { label: "Error", args: { hasError: true } },
  { label: "No authorizations", args: { sellingFixture: "none" } },
];

/**
 * Every state at once. `chrome={false}` because stacking N copies of `MarketplaceChrome` would
 * emit N `<main>` landmarks and fail axe's landmark-unique.
 */
export const Catalog: Story = {
  name: "Catalog (every state)",
  parameters: { controls: { disable: true } },
  render: (args) => (
    <div style={{ display: "flex", flexDirection: "column", gap: 32, padding: 24 }}>
      {CASES.map((c) => {
        const { sellingFixture, ...rest } = { ...args, ...c.args };
        return (
          <section key={c.label}>
            <div style={{ font: "600 13px var(--font-sans)", opacity: 0.7, margin: "0 0 8px" }}>
              {c.label}
            </div>
            <MkSettingsPage {...rest} chrome={false} selling={SELLING[sellingFixture]} />
          </section>
        );
      })}
    </div>
  ),
};
