import { test, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import NewShopBrowse from "./NewShopBrowse";
import { makeCards } from "./fixtures";

const SHOP_BUY_BUDGET = 1;
const SHOP_OPEN_BUDGET = 1;

test(`shop: buy a visible item in <= ${SHOP_BUY_BUDGET} click(s)`, async () => {
  const onBuyAsset = vi.fn();
  render(
    <NewShopBrowse
      cards={makeCards(6)}
      onBuyAsset={onBuyAsset}
      onOpenAsset={vi.fn()}
      onToggleFavorite={vi.fn()}
    />,
  );

  let clicks = 0;
  clicks++;
  await userEvent.click(screen.getAllByRole("button", { name: "Buy" })[0]!);

  expect(onBuyAsset).toHaveBeenCalledTimes(1);
  expect(onBuyAsset).toHaveBeenCalledWith("asset-0");
  expect(clicks).toBeLessThanOrEqual(SHOP_BUY_BUDGET);
});

test(`shop: open item detail in <= ${SHOP_OPEN_BUDGET} click(s)`, async () => {
  const onOpenAsset = vi.fn();
  render(
    <NewShopBrowse
      cards={makeCards(6)}
      onOpenAsset={onOpenAsset}
      onBuyAsset={vi.fn()}
      onToggleFavorite={vi.fn()}
    />,
  );

  let clicks = 0;
  clicks++;
  await userEvent.click(screen.getByRole("button", { name: "Golden Sneakers" }));

  expect(onOpenAsset).toHaveBeenCalledTimes(1);
  expect(clicks).toBeLessThanOrEqual(SHOP_OPEN_BUDGET);
});
