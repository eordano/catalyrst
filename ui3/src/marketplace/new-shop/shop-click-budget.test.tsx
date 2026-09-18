import { test, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import NewShopBrowse from "./NewShopBrowse";
import { makeCards } from "./fixtures";

const SHOP_BUY_BUDGET = 1;
const SHOP_OPEN_BUDGET = 1;

test(`shop: buy a visible item in <= ${SHOP_BUY_BUDGET} click(s) and open its detail in <= ${SHOP_OPEN_BUDGET} click(s)`, async () => {
  const onBuyAsset = vi.fn();
  const onOpenAsset = vi.fn();
  render(
    <NewShopBrowse
      cards={makeCards(6)}
      onBuyAsset={onBuyAsset}
      onOpenAsset={onOpenAsset}
      onToggleFavorite={vi.fn()}
    />,
  );

  let buyClicks = 0;
  buyClicks++;
  await userEvent.click(screen.getAllByRole("button", { name: "Buy" })[0]!);
  expect(onBuyAsset).toHaveBeenCalledTimes(1);
  expect(onBuyAsset).toHaveBeenCalledWith("asset-0");
  expect(buyClicks).toBeLessThanOrEqual(SHOP_BUY_BUDGET);

  let openClicks = 0;
  openClicks++;
  await userEvent.click(screen.getByRole("button", { name: "Golden Sneakers" }));
  expect(onOpenAsset).toHaveBeenCalledTimes(1);
  expect(openClicks).toBeLessThanOrEqual(SHOP_OPEN_BUDGET);
});
