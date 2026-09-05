import type { Meta, StoryObj } from "@storybook/react-vite";
import MarketplaceChrome from "./MarketplaceChrome";

const meta = {
  title: "Marketplace/Frames/MarketplaceChrome",
  component: MarketplaceChrome,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof MarketplaceChrome>;

export default meta;
type Story = StoryObj<typeof meta>;

const Body = () => (
  <div style={{ padding: 40, color: "rgba(255,255,255,.55)", fontSize: 14 }}>
    Page body renders here.
  </div>
);

export const Default: Story = {
  render: () => (
    <MarketplaceChrome active="overview">
      <Body />
    </MarketplaceChrome>
  ),
};

export const SignedIn: Story = {
  render: () => (
    <MarketplaceChrome active="overview" signedIn>
      <Body />
    </MarketplaceChrome>
  ),
};
