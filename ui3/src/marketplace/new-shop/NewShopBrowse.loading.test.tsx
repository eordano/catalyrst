import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import NewShopBrowse from "./NewShopBrowse";

afterEach(cleanup);

it("does not announce empty results while the first filtered page is pending", () => {
  const view = render(<NewShopBrowse cards={[]} loading />);
  expect(screen.getByText("Loading items\u2026")).toBeVisible();
  expect(screen.queryByText("No items match your filters.")).toBeNull();
  view.rerender(<NewShopBrowse cards={[]} />);
  expect(screen.getByText("No items match your filters.")).toBeVisible();
});
