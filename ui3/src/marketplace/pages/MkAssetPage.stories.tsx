import type { ComponentProps } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import MkAssetPage from "./MkAssetPage";

type Props = ComponentProps<typeof MkAssetPage>;

const SAMPLE_NFT: NonNullable<Props["nft"]> = {
  name: "Cyber Ronin Jacket",
  issuedId: 142,
  category: "upper_body",
  rarity: "legendary",
  bodyShape: "Unisex",
  isSmart: true,
  network: "ethereum",
  description:
    "A battle-worn techwear jacket forged in the neon districts. Reactive plating, an emissive collar trim, and a holographic clan sigil stitched across the back. Part of the Ronin drop \u{2014} one of only 100 ever minted.",
  owner: { address: "0x9f3c2b71a4d5e6f8c0b1a2d3e4f5a6b7c8d9e0a1", name: "neon.dcl" },
  collection: { name: "Neon Districts", address: "0xc04528c14c8ffd84c7c1fb6719b4a89853035cdd" },
  order: { price: "4250", issuedId: 142, expiresLabel: "Expires in 27 days" },
};

const SAMPLE_LISTINGS: NonNullable<Props["listings"]> = [
  { owner: "0x4d1f9a3c2e7b8d0f1a6c5b4e3d2f1a0b9c8d7e6f", name: "vapor.dcl", published: "Jun 04", expires: "in 24 days", issued: 88, price: "3990", listed: true },
  { owner: "0x7a2b1c0d9e8f3a4b5c6d7e8f9a0b1c2d3e4f5a6b", name: "kira", published: "Jun 11", expires: "in 30 days", issued: 203, price: "4100", listed: true },
  { owner: "0x1c8d7e6f5a4b3c2d1e0f9a8b7c6d5e4f3a2b1c0d", name: "0xshogun", published: "May 28", expires: "in 12 days", issued: 17, price: "4400", listed: true },
  { owner: "0x9e0f1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f", name: "mizu.dcl", published: "Jun 15", expires: "in 41 days", issued: 311, price: "4800", listed: false },
];

const meta = {
  title: "Marketplace/Pages/Asset",
  component: MkAssetPage,
  parameters: { layout: "fullscreen" },
  args: {
    nft: SAMPLE_NFT,
    listings: SAMPLE_LISTINGS,
  },
} satisfies Meta<typeof MkAssetPage>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  args: {},
};

export const NotForSale: Story = {
  args: {
    nft: {
      name: "Aurora Wings",
      issuedId: 7,
      category: "upper_body",
      rarity: "mythic",
      bodyShape: "Unisex",
      isSmart: false,
      network: "ethereum",
      description: "Iridescent angelic wings with a slow shimmering bloom. A festival-only mint.",
      owner: { address: "0x2b7c1d0e9f8a3b4c5d6e7f8a9b0c1d2e3f4a5b6c", name: "halo.dcl" },
      collection: { name: "Celestial Drop", address: "0xa1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0" },
      order: null,
    },
  },
};

export const EmptyListings: Story = {
  args: {
    emptyListings: true,
  },
};
