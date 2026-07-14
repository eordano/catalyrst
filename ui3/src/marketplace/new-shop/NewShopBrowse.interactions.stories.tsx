import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fireEvent, fn, userEvent, within } from "storybook/test";
import NewShopBrowse from "./NewShopBrowse";
import { filterGroups, makeCards } from "./fixtures";

const meta = {
  title: "Marketplace/NewShop/Pages/Browse/Interactions",
  component: NewShopBrowse,
  parameters: { layout: "fullscreen" },
  args: {
    activeTab: "all-assets",
    onTab: fn(),
    groups: filterGroups,
    cards: makeCards(6),
    onSort: fn(),
    onToggleOnSale: fn(),
    onOptionChange: fn(),
    onOpenAsset: fn(),
    onBuyAsset: fn(),
    onToggleFavorite: fn(),
  },
} satisfies Meta<typeof NewShopBrowse>;
export default meta;

type Story = StoryObj<typeof meta>;

export const ToggleOnSale: Story = {
  play: async ({ args, canvasElement }) => {
    const canvas = within(canvasElement);
    const sw = canvas.getByRole("switch", { name: "On Sale" });
    await expect(sw).toHaveAttribute("aria-checked", "true");
    await userEvent.click(sw);
    await expect(args.onToggleOnSale).toHaveBeenCalledWith(false);
  },
};

export const SortSelection: Story = {
  play: async ({ args, canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.click(canvas.getByRole("button", { name: /Sort by/i }));
    await userEvent.click(await canvas.findByRole("option", { name: "Price: Low to High" }));
    await expect(args.onSort).toHaveBeenCalledWith("Price: Low to High");
  },
};

export const FilterOption: Story = {
  play: async ({ args, canvasElement }) => {
    const canvas = within(canvasElement);
    await fireEvent.click(canvas.getByRole("checkbox", { name: "Emotes" }));
    await expect(args.onOptionChange).toHaveBeenCalledWith("category", "emotes", true);
  },
};

export const FavoriteGridCard: Story = {
  play: async ({ args, canvasElement }) => {
    const canvas = within(canvasElement);
    const hearts = canvas.getAllByRole("button", { name: "Add to favorites" });
    await userEvent.click(hearts[0]!);
    await expect(args.onToggleFavorite).toHaveBeenCalledWith("asset-0", true);
    await expect(canvas.getAllByRole("button", { name: "Remove from favorites" }).length).toBe(1);
  },
};

export const SearchSubmits: Story = {
  args: { onSearch: fn() },
  play: async ({ args, canvasElement }) => {
    const canvas = within(canvasElement);
    const input = canvas.getByRole("textbox", { name: /Search items/i });
    await userEvent.type(input, "  dragon  ");
    await expect(args.onSearch).not.toHaveBeenCalled();
    await userEvent.type(input, "{Enter}");
    await expect(args.onSearch).toHaveBeenCalledTimes(1);
    await expect(args.onSearch).toHaveBeenCalledWith("dragon");
  },
};

export const LoadingFeedback: Story = {
  args: { cards: makeCards(6), loading: true },
  play: async ({ canvasElement }) => {
    const region = canvasElement.querySelector(".nsbrowse__results");
    await expect(region).toHaveAttribute("aria-busy", "true");
    await expect(canvasElement.querySelector(".nsbrowse__loading")).toBeTruthy();
  },
};

export const Pagination: Story = {
  args: { cards: makeCards(6), page: 0, totalPages: 3, onPageChange: fn() },
  play: async ({ args, canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByText("Page 1 of 3")).toBeInTheDocument();
    await expect(canvas.getByRole("button", { name: "Previous page" })).toBeDisabled();
    const next = canvas.getByRole("button", { name: "Next page" });
    await expect(next).toBeEnabled();
    await userEvent.click(next);
    await expect(args.onPageChange).toHaveBeenCalledWith(1);
  },
};

export const PaginationLastPage: Story = {
  args: { cards: makeCards(6), page: 2, totalPages: 3, onPageChange: fn() },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByText("Page 3 of 3")).toBeInTheDocument();
    await expect(canvas.getByRole("button", { name: "Next page" })).toBeDisabled();
    await expect(canvas.getByRole("button", { name: "Previous page" })).toBeEnabled();
  },
};

export const EmptyState: Story = {
  args: { cards: [] },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByText("No items match your filters.")).toBeInTheDocument();
    await expect(canvas.queryByRole("button", { name: "Add to favorites" })).not.toBeInTheDocument();
    await expect(canvas.queryByRole("button", { name: "Clear filters" })).not.toBeInTheDocument();
  },
};

export const ControlledFavorites: Story = {
  args: { cards: makeCards(3), favorites: ["asset-1"], onToggleFavorite: fn() },
  play: async ({ args, canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getAllByRole("button", { name: "Remove from favorites" }).length).toBe(1);
    await userEvent.click(canvas.getAllByRole("button", { name: "Add to favorites" })[0]!);
    await expect(args.onToggleFavorite).toHaveBeenCalledWith("asset-0", true);
    await expect(canvas.getAllByRole("button", { name: "Remove from favorites" }).length).toBe(1);
  },
};

export const ClearFiltersRecovery: Story = {
  args: { cards: [], filtersActive: true, onClearFilters: fn() },
  play: async ({ args, canvasElement }) => {
    const canvas = within(canvasElement);
    const clear = canvas.getByRole("button", { name: "Clear filters" });
    await userEvent.click(clear);
    await expect(args.onClearFilters).toHaveBeenCalledTimes(1);
  },
};
