import type { ComponentProps } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import MkManageAssetPage from "./MkManageAssetPage";

type Props = ComponentProps<typeof MkManageAssetPage>;

const SAMPLE_ASSET: NonNullable<Props["asset"]> = {
  name: "Genesis Plaza Parcel",
  category: "parcel",
  coords: "12,-9",
  network: "Ethereum",
  owner: "0x9f3c\u{2026}7a21",
  description:
    "A premium parcel bordering Genesis Plaza, steps from the central hub and a major road. Flat terrain, ready to build.",
  proximities: [
    { type: "plaza", text: "1 away" },
    { type: "road", text: "Adjacent" },
    { type: "district", text: "3 away" },
  ],
};

const ESTATE_ASSET: NonNullable<Props["asset"]> = {
  name: "Skyline Estate",
  category: "estate",
  coords: "Estate #842",
  network: "Ethereum",
  owner: "0x9f3c\u{2026}7a21",
  description:
    "A 9-parcel estate near the fashion district. Currently unclaimed after a finished rental.",
  proximities: [{ type: "district", text: "Adjacent" }],
};

const ASSETS = { parcel: SAMPLE_ASSET, estate: ESTATE_ASSET } satisfies Record<
  string,
  NonNullable<Props["asset"]>
>;
type AssetKey = keyof typeof ASSETS;
const ASSET_KEYS = Object.keys(ASSETS) as AssetKey[];

const ORDERS = {
  listed: { price: "1,800", expiresAt: "Jul 20, 2026" },
  none: null,
} satisfies Record<string, Props["order"]>;
type OrderKey = keyof typeof ORDERS;
const ORDER_KEYS = Object.keys(ORDERS) as OrderKey[];

const RENTALS = {
  open: { status: "open", price: "25", expiration: "Jul 20, 2026", periods: "7 / 30 / 90" },
  executed: {
    status: "executed",
    price: "25",
    tenant: "0x4b2a\u{2026}d091",
    startRel: "2 months ago",
    startDate: "Apr 18, 2026",
    endRel: "in about 1 month",
    endDate: "Jul 18, 2026",
    endDateLong: "Saturday, July 18, 2026",
  },
  executedLongLease: {
    status: "executed",
    price: "40",
    tenant: "0x4b2a\u{2026}d091",
    startRel: "1 month ago",
    startDate: "May 18, 2026",
    endRel: "in 5 months",
    endDate: "Nov 18, 2026",
  },
  claimable: { status: "claimable" },
  none: null,
} satisfies Record<string, Props["rental"]>;
type RentalKey = keyof typeof RENTALS;
const RENTAL_KEYS = Object.keys(RENTALS) as RentalKey[];

type ManageStoryArgs = {
  assetKind: AssetKey;
  orderKind: OrderKey;
  rentalKind: RentalKey;
  showUpgradeWarning: boolean;
  locked: boolean;
};

const meta = {
  title: "Marketplace/Pages/Manage asset",
  component: MkManageAssetPage,
  parameters: { layout: "fullscreen" },
  argTypes: {
    assetKind: { control: "select", options: ASSET_KEYS },
    orderKind: { control: "select", options: ORDER_KEYS },
    rentalKind: { control: "select", options: RENTAL_KEYS },
    showUpgradeWarning: { control: "boolean" },
    locked: { control: "boolean" },
  },
  args: {
    assetKind: "parcel",
    orderKind: "listed",
    rentalKind: "open",
    showUpgradeWarning: false,
    locked: false,
  },
  render: ({ assetKind, orderKind, rentalKind, ...rest }) => (
    <MkManageAssetPage
      {...rest}
      asset={ASSETS[assetKind]}
      order={ORDERS[orderKind]}
      rental={RENTALS[rentalKind]}
    />
  ),
} satisfies Meta<ManageStoryArgs>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

type CatalogEntry = { label: string } & ManageStoryArgs;

const CATALOG: CatalogEntry[] = [
  { label: "listed + open rental", assetKind: "parcel", orderKind: "listed", rentalKind: "open", showUpgradeWarning: false, locked: false },
  { label: "no listings", assetKind: "parcel", orderKind: "none", rentalKind: "none", showUpgradeWarning: false, locked: false },
  { label: "selling (order, no rental)", assetKind: "parcel", orderKind: "listed", rentalKind: "none", showUpgradeWarning: false, locked: false },
  { label: "active rental", assetKind: "parcel", orderKind: "none", rentalKind: "executed", showUpgradeWarning: false, locked: false },
  { label: "claim back (estate)", assetKind: "estate", orderKind: "none", rentalKind: "claimable", showUpgradeWarning: false, locked: false },
  { label: "upgrade warning", assetKind: "parcel", orderKind: "listed", rentalKind: "open", showUpgradeWarning: true, locked: false },
  { label: "LAND locked", assetKind: "parcel", orderKind: "none", rentalKind: "executedLongLease", showUpgradeWarning: false, locked: true },
];

export const Catalog: Story = {
  name: "Catalog (every state)",
  parameters: { controls: { disable: true } },
  render: () => (
    <div style={{ display: "flex", flexDirection: "column", gap: 32, padding: 24 }}>
      {CATALOG.map((entry) => (
        <section key={entry.label}>
          <div style={{ font: "600 13px var(--font-sans)", opacity: 0.7, margin: "0 0 8px" }}>
            {entry.label}
          </div>
          <MkManageAssetPage
            chrome={false}
            asset={ASSETS[entry.assetKind]}
            order={ORDERS[entry.orderKind]}
            rental={RENTALS[entry.rentalKind]}
            showUpgradeWarning={entry.showUpgradeWarning}
            locked={entry.locked}
          />
        </section>
      ))}
    </div>
  ),
};
