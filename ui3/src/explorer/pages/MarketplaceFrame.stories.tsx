import type { Meta, StoryObj } from "@storybook/react-vite";
import ExploreChrome from "../frames/ExploreChrome";
import MarketplaceFrame from "./MarketplaceFrame";

const meta = {
  title: "Explorer/Pages/Marketplace",
  component: MarketplaceFrame,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof MarketplaceFrame>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: () => (
    <ExploreChrome user="CosmicLux">
      <MarketplaceFrame src="about:blank" />
    </ExploreChrome>
  ),
};
