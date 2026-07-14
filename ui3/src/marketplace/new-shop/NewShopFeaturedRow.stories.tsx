import type { Meta, StoryObj } from "@storybook/react-vite";
import { fn } from "storybook/test";
import NewShopFeaturedRow from "./NewShopFeaturedRow";
import NewShopAssetCard from "./NewShopAssetCard";
import { makeCards } from "./fixtures";

const meta = {
  title: "Marketplace/NewShop/FeaturedRow",
  component: NewShopFeaturedRow,
  parameters: { layout: "fullscreen" },
  decorators: [
    (Story) => (
      <div className="mk" style={{ background: "var(--lm-bg)", padding: 24 }}>
        <Story />
      </div>
    ),
  ],
  args: { title: "Featured", onViewAll: fn(), onPrev: fn(), onNext: fn() },
} satisfies Meta<typeof NewShopFeaturedRow>;
export default meta;

type Story = StoryObj<typeof meta>;

export const Default: Story = {
  render: (args) => (
    <NewShopFeaturedRow {...args}>
      {makeCards(8).map((c) => (
        <NewShopAssetCard
          key={c.id}
          name={c.name}
          meta={c.meta}
          price={c.price}
          rarity={c.rarity}
          network={c.network}
          onBuy={fn()}
          onOpen={fn()}
          onToggleFavorite={fn()}
        />
      ))}
    </NewShopFeaturedRow>
  ),
};
