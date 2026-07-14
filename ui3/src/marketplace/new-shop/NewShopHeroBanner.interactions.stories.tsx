import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fn, userEvent, within } from "storybook/test";
import NewShopHeroBanner from "./NewShopHeroBanner";

const meta = {
  title: "Marketplace/NewShop/HeroBanner/Interactions",
  component: NewShopHeroBanner,
  parameters: { layout: "padded" },
  decorators: [
    (Story) => (
      <div className="mk" style={{ maxWidth: 560, background: "var(--lm-bg)", padding: 16 }}>
        <Story />
      </div>
    ),
  ],
} satisfies Meta<typeof NewShopHeroBanner>;
export default meta;

type Story = StoryObj<typeof meta>;

export const CtaFires: Story = {
  args: { title: "Best Rated Emotes", cta: "Shop emotes", onCta: fn() },
  play: async ({ args, canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.click(canvas.getByRole("button", { name: "Shop emotes" }));
    await expect(args.onCta).toHaveBeenCalledTimes(1);
  },
};

export const NoCtaNoButton: Story = {
  args: { title: "Week Selected Outfits" },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.queryByRole("button")).not.toBeInTheDocument();
  },
};
