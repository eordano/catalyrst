import type { ComponentProps } from "react";
import type { Meta, StoryObj } from "@storybook/react-vite";
import AssetActionLayout from "./AssetActionLayout";
import AssetPreviewTile from "../components/AssetPreviewTile";
import ManaMark from "../../atoms/ManaMark";

type LayoutProps = ComponentProps<typeof AssetActionLayout>;

const Buttons = ({ submit = "List for sale" }: { submit?: string }) => (
  <div style={{ display: "flex", gap: 14 }}>
    <button type="button" style={{ padding: "10px 20px", borderRadius: 999 }}>Cancel</button>
    <button type="button" style={{ padding: "10px 20px", borderRadius: 999 }}>{submit}</button>
  </div>
);

const CheckGlyph = (
  <svg viewBox="0 0 24 24" width="34" height="34" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round">
    <path d="M20 6 9 17l-5-5" />
  </svg>
);

const MEDIA = {
  none: undefined,
  legendary: <AssetPreviewTile rarity="legendary" />,
  mythicPlate: <AssetPreviewTile rarity="mythic" figure="plate" chipPosition="bottom" />,
  epicInset: <AssetPreviewTile rarity="epic" figure="inset" label={null} />,
};

const SUBTITLE = {
  none: undefined,
  list: <>Set a price and expiration date for <b>Cyber Ronin Jacket</b>.</>,
  bid: <>Set a price and expiration date for your bid on <b>Pioneer Jacket</b>.</>,
  transferBlocked: <><b>Cyber Ronin Jacket</b> can&apos;t be transferred because it&apos;s on sale.</>,
  listingRemoved: "Your listing has been removed. The item is no longer for sale.",
};

const WARNING = {
  none: undefined,
  gasFree: (
    <span style={{ fontSize: 13, color: "var(--ink-6)" }}>
      Pay with Polygon MANA to have gas fees covered for you by the DAO.
    </span>
  ),
};

const ICON = { none: undefined, check: CheckGlyph };

const BACK_LABEL = { back: "Back", none: null };

const ACTIONS = {
  list: <Buttons />,
  bid: <Buttons submit="Bid" />,
  transfer: <Buttons submit="Transfer" />,
  viewItem: <Buttons submit="View item" />,
};

const meta = {
  title: "Marketplace/Frames/AssetActionLayout",
  component: AssetActionLayout,
  parameters: { layout: "fullscreen" },
  argTypes: {
    theme: { control: "inline-radio", options: ["light", "dark"] },
    variant: { control: "inline-radio", options: ["row", "status"] },
    title: { control: "text" },
    subtitle: { control: "select", options: Object.keys(SUBTITLE), mapping: SUBTITLE },
    subtitleTone: { control: "inline-radio", options: ["default", "danger"] },
    warning: { control: "select", options: Object.keys(WARNING), mapping: WARNING },
    media: { control: "select", options: Object.keys(MEDIA), mapping: MEDIA },
    icon: { control: "select", options: Object.keys(ICON), mapping: ICON },
    iconTone: { control: "select", options: ["neutral", "success", "danger"] },
    backLabel: { control: "select", options: Object.keys(BACK_LABEL), mapping: BACK_LABEL },
    hideBack: { control: "boolean" },
    center: { control: "boolean" },
    maxWidth: { control: "number" },
    children: { control: "select", options: Object.keys(ACTIONS), mapping: ACTIONS },
  },
  args: {
    theme: "light",
    variant: "row",
    title: "List for sale",
    subtitle: "list",
    subtitleTone: "default",
    warning: "none",
    media: "legendary",
    icon: "none",
    iconTone: "neutral",
    backLabel: "back",
    hideBack: false,
    center: false,
    maxWidth: 1100,
    children: "list",
  },
} satisfies Meta<typeof AssetActionLayout>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {};

/** A different component: the compact tile used inside modals, not a layout at all. */
export const SmallTile: Story = {
  parameters: { controls: { disable: true } },
  render: () => (
    <div style={{ padding: 40, background: "var(--panel)" }}>
      <AssetPreviewTile rarity="legendary" label={null} size={55} radius={6}>
        <ManaMark size={22} />
      </AssetPreviewTile>
    </div>
  ),
};

const CASES: { label: string; args: Partial<LayoutProps> }[] = [
  {
    label: "Light \u{B7} list for sale",
    args: {
      theme: "light",
      title: "List for sale",
      subtitle: SUBTITLE.list,
      media: MEDIA.legendary,
      children: ACTIONS.list,
    },
  },
  {
    label: "Dark \u{B7} centered, with warning",
    args: {
      theme: "dark",
      center: true,
      maxWidth: 980,
      title: "Place a bid",
      subtitle: SUBTITLE.bid,
      warning: WARNING.gasFree,
      media: MEDIA.mythicPlate,
      children: ACTIONS.bid,
    },
  },
  {
    label: "Icon back \u{B7} danger subtitle",
    args: {
      theme: "dark",
      backLabel: null,
      maxWidth: 920,
      title: "Transfer Wearable",
      subtitle: SUBTITLE.transferBlocked,
      subtitleTone: "danger",
      media: MEDIA.epicInset,
      children: ACTIONS.transfer,
    },
  },
  {
    label: "Status variant",
    args: {
      theme: "dark",
      variant: "status",
      hideBack: true,
      iconTone: "success",
      icon: CheckGlyph,
      title: "Listing removed",
      subtitle: SUBTITLE.listingRemoved,
      children: ACTIONS.viewItem,
    },
  },
];

/**
 * Every layout at once. `Default` flips between them from the Controls panel; this keeps all four
 * in the render + a11y + visual-diff gates, since the `status` variant and the icon-back header
 * are structurally different subtrees.
 */
export const Catalog: Story = {
  name: "Catalog (every layout)",
  parameters: { controls: { disable: true } },
  render: () => (
    <div style={{ display: "flex", flexDirection: "column", gap: 32 }}>
      {CASES.map((c) => (
        <section key={c.label}>
          <div style={{ font: "600 13px var(--font-sans)", opacity: 0.7, margin: "0 0 8px", padding: "0 24px" }}>
            {c.label}
          </div>
          <AssetActionLayout {...c.args} />
        </section>
      ))}
    </div>
  ),
};
