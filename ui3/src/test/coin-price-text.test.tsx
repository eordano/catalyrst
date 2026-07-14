import { describe, expect, it } from "vitest";
import { render } from "@testing-library/react";

import { Coin } from "../atoms/icons";
import AssetCard from "../marketplace/components/AssetCard";

describe("Coin icon / price text", () => {
  it("Coin renders no text nodes (both ring variants)", () => {
    const ringed = render(<Coin size={13} />);
    expect(ringed.container.textContent).toBe("");
    const plain = render(<Coin size={13} ring={false} />);
    expect(plain.container.textContent).toBe("");
    expect(ringed.container.querySelectorAll("circle").length).toBe(2);
    expect(ringed.container.querySelector("path")).not.toBeNull();
    expect(ringed.container.querySelector("text")).toBeNull();
  });

  it("a 1-credit card copies as singular '1 credit' with no leading M", () => {
    const { container } = render(
      <AssetCard name="Cigar" price={1} unit="credits" />,
    );
    const price = container.querySelector(".ac__price");
    expect(price).not.toBeNull();
    const text = (price!.textContent ?? "").replace(/\s+/g, " ").trim();
    expect(text).toBe("1 credit");
    expect(text.startsWith("M")).toBe(false);
  });

  it("a multi-credit card copies as plural 'credits'", () => {
    const { container } = render(
      <AssetCard name="Cigar" price={2} unit="credits" />,
    );
    const price = container.querySelector(".ac__price");
    const text = (price!.textContent ?? "").replace(/\s+/g, " ").trim();
    expect(text).toBe("2 credits");
  });
});
