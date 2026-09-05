import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fireEvent, fn, userEvent, within } from "storybook/test";
import NewShopFeaturedRow from "./NewShopFeaturedRow";
import NewShopAssetCard from "./NewShopAssetCard";
import { makeCards } from "./fixtures";

const meta = {
  title: "Marketplace/NewShop/FeaturedRow/Interactions",
  component: NewShopFeaturedRow,
  parameters: { layout: "fullscreen" },
  decorators: [
    (Story) => (
      <div className="mk" style={{ background: "var(--lm-bg)", padding: 24, width: 480 }}>
        <Story />
      </div>
    ),
  ],
} satisfies Meta<typeof NewShopFeaturedRow>;
export default meta;

type Story = StoryObj<typeof meta>;

function overflow(el: HTMLElement, { scrollWidth, clientWidth, scrollLeft }: { scrollWidth: number; clientWidth: number; scrollLeft: number }) {
  Object.defineProperty(el, "scrollWidth", { configurable: true, value: scrollWidth });
  Object.defineProperty(el, "clientWidth", { configurable: true, value: clientWidth });
  Object.defineProperty(el, "scrollLeft", { configurable: true, writable: true, value: scrollLeft });
}

export const ArrowsScrollAndDisable: Story = {
  args: { title: "Featured", onViewAll: fn() },
  render: (args) => (
    <NewShopFeaturedRow {...args}>
      {makeCards(8).map((c) => (
        <NewShopAssetCard key={c.id} name={c.name} price={c.price} rarity={c.rarity} onOpen={fn()} onToggleFavorite={fn()} />
      ))}
    </NewShopFeaturedRow>
  ),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const track = canvasElement.querySelector(".nsfeat__track") as HTMLElement;
    const scrollBySpy = fn();
    (track as unknown as { scrollBy: typeof scrollBySpy }).scrollBy = scrollBySpy;

    overflow(track, { scrollWidth: 1600, clientWidth: 400, scrollLeft: 0 });
    fireEvent.scroll(track);
    await expect(canvas.getByRole("button", { name: "Previous" })).toBeDisabled();
    const next = canvas.getByRole("button", { name: "Next" });
    await expect(next).toBeEnabled();

    await userEvent.click(next);
    await expect(scrollBySpy).toHaveBeenCalled();

    overflow(track, { scrollWidth: 1600, clientWidth: 400, scrollLeft: 1200 });
    fireEvent.scroll(track);
    await expect(canvas.getByRole("button", { name: "Next" })).toBeDisabled();
    await expect(canvas.getByRole("button", { name: "Previous" })).toBeEnabled();
  },
};

export const ViewAllFires: Story = {
  args: { title: "Featured", onViewAll: fn() },
  render: (args) => (
    <NewShopFeaturedRow {...args}>
      {makeCards(4).map((c) => (
        <NewShopAssetCard key={c.id} name={c.name} price={c.price} rarity={c.rarity} onOpen={fn()} onToggleFavorite={fn()} />
      ))}
    </NewShopFeaturedRow>
  ),
  play: async ({ args, canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.click(canvas.getByRole("button", { name: "View all" }));
    await expect(args.onViewAll).toHaveBeenCalledTimes(1);
  },
};
