import { expect, it } from "vitest";
import { render } from "@testing-library/react";

import { Coin } from "../atoms/icons";
import AssetCard from "../marketplace/components/AssetCard";

const priceText = (price: number) => {
  const { container } = render(<AssetCard name="Cigar" price={price} unit="credits" />);
  const price$ = container.querySelector(".ac__price");
  expect(price$).not.toBeNull();
  return (price$!.textContent ?? "").replace(/\s+/g, " ").trim();
};

it("the Coin icon carries no text and credit prices copy as singular or plural with no leading M", () => {
  expect(render(<Coin size={13} />).container.textContent).toBe("");
  expect(render(<Coin size={13} ring={false} />).container.textContent).toBe("");
  expect(priceText(1)).toBe("1 credit");
  expect(priceText(2)).toBe("2 credits");
});
