import type { ComponentProps } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import MkTransferPage from "./MkTransferPage";

type Props = ComponentProps<typeof MkTransferPage>;
type TransferStatus = NonNullable<Props["status"]>;
type Nft = NonNullable<Props["nft"]>;

/** The two asset shapes the page has ever been shown with, picked by name. */
const NFTS = {
  wearable: {
    contractAddress: "0x09f1c2\u{2026}b3d4",
    tokenId: "104500",
    name: "Cyber Ronin Jacket",
    category: "wearable",
    rarity: "legendary",
    network: "polygon",
  },
  emote: {
    contractAddress: "0x6a3b\u{2026}f1c2",
    tokenId: "88",
    name: "Crab Rave",
    category: "emote",
    rarity: "epic",
    network: "polygon",
  },
} satisfies Record<string, Nft>;

type NftKey = keyof typeof NFTS;
const NFT_KEYS = Object.keys(NFTS) as NftKey[];

const STATUSES = [
  "form",
  "for_sale",
  "invalid_owner",
  "transferring",
  "success",
] satisfies TransferStatus[];

const SAMPLE_TX_HASH = "0x7c9a4f2e1b6d8c0a3e5f7b9d1c2a4e6f8b0d2c4a6e8f0b2d4c6a8e0f2b4d6c8a";

/** `nftKind` names a fixture; `status` and `txHash` stay real props. */
type TransferStoryArgs = {
  nftKind: NftKey;
  status: TransferStatus;
  txHash: string;
};

const meta = {
  title: "Marketplace/Pages/Transfer",
  component: MkTransferPage,
  parameters: { layout: "fullscreen" },
  argTypes: {
    nftKind: { control: "select", options: NFT_KEYS },
    status: { control: "select", options: STATUSES },
    txHash: { control: "text" },
  },
  args: { nftKind: "wearable", status: "form", txHash: SAMPLE_TX_HASH },
  render: ({ nftKind, ...rest }) => <MkTransferPage {...rest} nft={NFTS[nftKind]} />,
} satisfies Meta<TransferStoryArgs>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

/**
 * Every `status` at once. `Default` flips between them from the Controls panel; this story keeps
 * all five in the render + a11y + visual-diff gates, since the error, pending and success states
 * replace the form with entirely different subtrees. `chrome={false}` because stacking N copies of
 * `MarketplaceChrome` would emit N `<main>` landmarks and fail axe's landmark-unique.
 *
 * Exactly one entry may be in the `form` status: the recipient input hardcodes
 * `id="mktransferpage-addr"`, so a second form instance would silently re-point its `<label>` at
 * the first one's input. The emote fixture -- which only changes the title and the preview tile,
 * not the structure -- is reachable from the `nftKind` control instead.
 */
export const Catalog: Story = {
  name: "Catalog (every status)",
  parameters: { controls: { disable: true } },
  render: () => (
    <div style={{ display: "flex", flexDirection: "column", gap: 32, padding: 24 }}>
      {STATUSES.map((status) => (
        <section key={status}>
          <div style={{ font: "600 13px var(--font-sans)", opacity: 0.7, margin: "0 0 8px" }}>
            {status}
          </div>
          <MkTransferPage
            chrome={false}
            nft={NFTS.wearable}
            status={status}
            txHash={SAMPLE_TX_HASH}
          />
        </section>
      ))}
    </div>
  ),
};
